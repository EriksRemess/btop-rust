mod cli;
mod collect;
mod config;
mod gpu;
mod logger;
mod render;
mod terminal;
mod theme;
mod units;

use std::ffi::{CStr, CString};
use std::io::{self, IsTerminal};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;
use std::time::{Duration, Instant};

use cli::{Action, Cli};
use collect::Collector;
use config::Config;
use render::{AppState, Renderer};
use terminal::Terminal;

const SIGNAL_QUIT: u32 = 1 << 0;
const SIGNAL_SUSPEND: u32 = 1 << 1;
const SIGNAL_REDRAW: u32 = 1 << 2;
const SIGNAL_RELOAD: u32 = 1 << 3;
static PENDING_SIGNALS: AtomicU32 = AtomicU32::new(0);

struct CollectionClock {
    interval: Duration,
    deadline: Instant,
}

impl CollectionClock {
    fn new(now: Instant, update_ms: u64) -> Self {
        Self {
            interval: Duration::from_millis(update_ms),
            deadline: now,
        }
    }

    fn sync_interval(&mut self, now: Instant, update_ms: u64) {
        let interval = Duration::from_millis(update_ms);
        if self.interval != interval {
            self.interval = interval;
            self.deadline = now + interval;
        }
    }

    fn collection_due(&self, now: Instant) -> bool {
        now >= self.deadline
    }

    fn collection_finished(&mut self, now: Instant) {
        self.deadline = now + self.interval;
    }

    fn input_deadline(&self, now: Instant) -> Instant {
        if self.deadline > now {
            self.deadline
        } else {
            now + self.interval
        }
    }
}

extern "C" fn signal_handler(signal: i32) {
    let flag = match signal {
        2 => SIGNAL_QUIT,
        20 => SIGNAL_SUSPEND,
        18 | 28 => SIGNAL_REDRAW,
        12 => SIGNAL_RELOAD,
        _ => 0,
    };
    PENDING_SIGNALS.fetch_or(flag, Ordering::Relaxed);
}

extern "C" fn crash_handler(signal_number: i32) {
    // SAFETY: restore_after_crash uses only libc terminal/write calls and a
    // terminal snapshot published before raw mode became active.
    unsafe {
        terminal::restore_after_crash();
        set_signal_handler(signal_number, 0);
        raise_signal(signal_number);
    }
}

unsafe fn set_signal_handler(signal_number: i32, handler: usize) -> usize {
    unsafe extern "C" {
        fn signal(signal: i32, handler: usize) -> usize;
    }
    unsafe { signal(signal_number, handler) }
}

unsafe fn raise_signal(signal_number: i32) -> i32 {
    unsafe extern "C" {
        fn raise(signal: i32) -> i32;
    }
    unsafe { raise(signal_number) }
}

fn install_signal_handlers() -> Result<(), String> {
    for signal_number in [2, 20, 18, 28, 10, 12] {
        if unsafe { set_signal_handler(signal_number, signal_handler as *const () as usize) }
            == usize::MAX
        {
            return Err(format!(
                "could not install handler for signal {signal_number}: {}",
                io::Error::last_os_error()
            ));
        }
    }
    for signal_number in [11, 6, 5, 7, 4] {
        if unsafe { set_signal_handler(signal_number, crash_handler as *const () as usize) }
            == usize::MAX
        {
            return Err(format!(
                "could not install crash handler for signal {signal_number}: {}",
                io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

fn current_tty() -> Option<String> {
    unsafe extern "C" {
        fn ttyname(fd: i32) -> *const std::os::raw::c_char;
    }
    let name = unsafe { ttyname(0) };
    (!name.is_null()).then(|| {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    })
}

fn auto_tty_mode(name: Option<&str>) -> bool {
    name.is_some_and(|name| name.starts_with("/dev/tty"))
}

fn main() -> ExitCode {
    match run() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            logger::error(&error);
            eprintln!("\x1b[1;31merror:\x1b[0m {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<u8, String> {
    let runtime_started = Instant::now();
    let cli = Cli::parse(std::env::args().skip(1))?;
    match cli.action {
        Some(Action::Help) => {
            cli::print_usage();
            return Ok(0);
        }
        Some(Action::Version { verbose }) => {
            cli::print_version(verbose);
            return Ok(0);
        }
        Some(Action::DefaultConfig) => {
            print!("{}", Config::default_file());
            return Ok(0);
        }
        None => {}
    }

    if !io::stdout().is_terminal() || !io::stdin().is_terminal() {
        return Err("btop requires an interactive terminal".into());
    }

    let mut config = Config::load(cli.config_file.as_deref())?;
    config.apply_cli(&cli);
    let tty_name = current_tty();
    if cli.force_tty.is_none() && !config.tty_mode && auto_tty_mode(tty_name.as_deref()) {
        config.tty_mode = true;
    }
    logger::init();
    logger::set_level(config.value("log_level").unwrap_or("WARNING"), cli.debug);
    if cli.debug {
        logger::debug("Running in DEBUG mode!");
    }
    logger::info(&format!(
        "Logger set to {}",
        if cli.debug {
            "DEBUG"
        } else {
            config.value("log_level").unwrap_or("WARNING")
        }
    ));
    for warning in &config.warnings {
        logger::warning(warning);
    }
    ensure_utf8_locale(cli.force_utf)?;
    install_signal_handlers()?;
    let mut terminal = Terminal::enter(!config.disable_mouse, config.terminal_sync)?;
    if let Some(tty_name) = tty_name.as_deref() {
        logger::info(&format!("Running on {tty_name}"));
    }
    if cli.force_tty.is_some() {
        logger::debug("TTY mode set via command line");
    } else if config.bool_value("force_tty").unwrap_or(false) {
        logger::debug("TTY mode set via config");
    } else if auto_tty_mode(tty_name.as_deref()) {
        logger::debug("Auto detect real TTY");
    }
    logger::debug(&format!("TTY mode enabled: {}", config.tty_mode));
    let mut collector = Collector::new(&config)?;
    let mut renderer = Renderer::new();
    let mut app = AppState::new(config);
    app.set_debug(cli.debug);
    let mut collection_clock = CollectionClock::new(Instant::now(), app.config.update_ms);

    let exit_code = 'main: loop {
        logger::set_level(
            app.config.value("log_level").unwrap_or("WARNING"),
            cli.debug,
        );
        match handle_pending_signals(&mut terminal, &mut app, &cli)? {
            SignalOutcome::Quit => break 'main 0,
            SignalOutcome::Redraw | SignalOutcome::None => {}
        }
        let now = Instant::now();
        collection_clock.sync_interval(now, app.config.update_ms);
        let size = terminal.size()?;
        if app.should_collect() && collection_clock.collection_due(now) {
            let sample = collector.collect(&app.config, app.detailed_pid())?;
            app.update(sample);
            collection_clock.collection_finished(Instant::now());
        }
        let needed = render::minimum_size(&app.config, &app.sample.gpus);
        if size.cols < needed.cols || size.rows < needed.rows {
            terminal.draw(&render::too_small(size, needed))?;
        } else {
            terminal.draw(&renderer.render(size, &mut app))?;
        }

        let deadline = collection_clock.input_deadline(Instant::now());
        loop {
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            let wait = (deadline - now).min(Duration::from_millis(100));
            match terminal.read_key(wait)? {
                Some(key) => {
                    if key == terminal::Key::CtrlR {
                        let _ = app.config.reload();
                        app.config.apply_cli(&cli);
                        terminal
                            .apply_settings(!app.config.disable_mouse, app.config.terminal_sync)?;
                        app.needs_redraw = true;
                        break;
                    }
                    if key == terminal::Key::CtrlZ {
                        terminal.leave()?;
                        suspend_process()?;
                        terminal =
                            Terminal::enter(!app.config.disable_mouse, app.config.terminal_sync)?;
                        app.needs_redraw = true;
                        break;
                    }
                    if app.handle_key(key) {
                        break 'main 0;
                    }
                    terminal.apply_settings(!app.config.disable_mouse, app.config.terminal_sync)?;
                    if app.needs_redraw {
                        app.needs_redraw = false;
                        break;
                    }
                }
                None => thread::yield_now(),
            }
            match handle_pending_signals(&mut terminal, &mut app, &cli)? {
                SignalOutcome::Quit => break 'main 0,
                SignalOutcome::Redraw => break,
                SignalOutcome::None => {}
            }
        }
    };
    terminal.leave()?;
    let _ = app.config.save();
    logger::info(&format!(
        "Quitting! Runtime: {}",
        units::duration(runtime_started.elapsed().as_secs())
    ));
    Ok(exit_code)
}

fn ensure_utf8_locale(force: bool) -> Result<(), String> {
    unsafe extern "C" {
        fn setlocale(
            category: i32,
            locale: *const std::os::raw::c_char,
        ) -> *mut std::os::raw::c_char;
    }
    const LC_ALL: i32 = 6;
    let set = |locale: &str| -> Option<String> {
        let locale = CString::new(locale).ok()?;
        let selected = unsafe { setlocale(LC_ALL, locale.as_ptr()) };
        (!selected.is_null()).then(|| {
            unsafe { CStr::from_ptr(selected) }
                .to_string_lossy()
                .into_owned()
        })
    };
    if set("").is_some_and(|locale| locale_is_utf8(&locale) && !locale.contains(';')) {
        return Ok(());
    }
    for variable in ["LANG", "LC_ALL", "LC_CTYPE"] {
        if let Ok(locale) = std::env::var(variable)
            && locale_is_utf8(&locale)
            && set(&locale).is_some()
        {
            return Ok(());
        }
    }
    if force {
        Ok(())
    } else {
        Err("No UTF-8 locale detected!\nUse --force-utf argument to force start if you're sure your terminal can handle it.".into())
    }
}

fn locale_is_utf8(locale: &str) -> bool {
    locale
        .replace('-', "")
        .to_ascii_uppercase()
        .ends_with("UTF8")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SignalOutcome {
    None,
    Redraw,
    Quit,
}

fn handle_pending_signals(
    terminal: &mut Terminal,
    app: &mut AppState,
    cli: &Cli,
) -> Result<SignalOutcome, String> {
    let pending = PENDING_SIGNALS.swap(0, Ordering::Relaxed);
    if pending & SIGNAL_QUIT != 0 {
        return Ok(SignalOutcome::Quit);
    }
    let mut redraw = pending & SIGNAL_REDRAW != 0;
    if pending & SIGNAL_RELOAD != 0 {
        let _ = app.config.reload();
        app.config.apply_cli(cli);
        terminal.apply_settings(!app.config.disable_mouse, app.config.terminal_sync)?;
        redraw = true;
    }
    if pending & SIGNAL_SUSPEND != 0 {
        terminal.leave()?;
        suspend_process()?;
        *terminal = Terminal::enter(!app.config.disable_mouse, app.config.terminal_sync)?;
        redraw = true;
    }
    if redraw {
        app.needs_redraw = true;
        Ok(SignalOutcome::Redraw)
    } else {
        Ok(SignalOutcome::None)
    }
}

fn suspend_process() -> Result<(), String> {
    unsafe extern "C" {
        fn raise(signal: i32) -> i32;
    }
    const SIGSTOP: i32 = 19;
    if unsafe { raise(SIGSTOP) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error().to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{CollectionClock, auto_tty_mode, locale_is_utf8};
    use std::time::{Duration, Instant};

    #[test]
    fn recognizes_the_utf8_locale_spellings_used_by_btop() {
        assert!(locale_is_utf8("lv_LV.UTF-8"));
        assert!(locale_is_utf8("C.utf8"));
        assert!(!locale_is_utf8("C"));
        assert!(!locale_is_utf8("en_US.ISO-8859-1"));
    }

    #[test]
    fn input_redraws_do_not_advance_the_collection_clock() {
        let started = Instant::now();
        let mut clock = CollectionClock::new(started, 1_000);
        assert!(clock.collection_due(started));

        clock.collection_finished(started);
        for elapsed_ms in [1, 10, 100, 250, 500, 999] {
            assert!(
                !clock.collection_due(started + Duration::from_millis(elapsed_ms)),
                "an input redraw at {elapsed_ms}ms must not collect a new sample"
            );
        }
        assert!(clock.collection_due(started + Duration::from_millis(1_000)));
    }

    #[test]
    fn changing_update_ms_restarts_the_collection_interval() {
        let started = Instant::now();
        let mut clock = CollectionClock::new(started, 1_000);
        clock.collection_finished(started);

        let changed = started + Duration::from_millis(400);
        clock.sync_interval(changed, 2_000);
        assert!(!clock.collection_due(changed + Duration::from_millis(1_999)));
        assert!(clock.collection_due(changed + Duration::from_millis(2_000)));
    }

    #[test]
    fn auto_tty_mode_matches_btop_real_console_detection() {
        assert!(auto_tty_mode(Some("/dev/tty1")));
        assert!(auto_tty_mode(Some("/dev/ttyS0")));
        assert!(!auto_tty_mode(Some("/dev/pts/4")));
        assert!(!auto_tty_mode(None));
    }
}
