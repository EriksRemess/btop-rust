#[cfg(unix)]
use std::ffi::{CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::Write;
#[cfg(unix)]
use std::os::raw::{c_char, c_int, c_long};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const ONE_MEBIBYTE: u64 = 1024 * 1024;

// musl switched 32-bit targets to a 64-bit time_t while c_long remains
// 32-bit. Other currently supported Unix targets use c_long for time_t.
#[cfg(all(unix, target_pointer_width = "32", target_env = "musl"))]
type TimeT = i64;
#[cfg(all(unix, not(all(target_pointer_width = "32", target_env = "musl"))))]
type TimeT = c_long;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Level {
    Disabled,
    Error,
    Warning,
    Info,
    Debug,
}

struct State {
    level: Level,
    path: Option<PathBuf>,
    wrote_header: bool,
}

fn state() -> &'static Mutex<State> {
    static STATE: OnceLock<Mutex<State>> = OnceLock::new();
    STATE.get_or_init(|| {
        Mutex::new(State {
            level: Level::Error,
            path: None,
            wrote_header: false,
        })
    })
}

fn lock_state() -> std::sync::MutexGuard<'static, State> {
    state()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

pub fn init() {
    let Some(path) = log_path() else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    lock_state().path = Some(path);
}

pub fn set_level(level: &str, debug: bool) {
    let level = if debug {
        Level::Debug
    } else {
        match level {
            "DISABLED" => Level::Disabled,
            "ERROR" => Level::Error,
            "WARNING" => Level::Warning,
            "INFO" => Level::Info,
            "DEBUG" => Level::Debug,
            _ => Level::Warning,
        }
    };
    lock_state().level = level;
}

pub fn error(message: &str) {
    write(Level::Error, message);
}

pub fn warning(message: &str) {
    write(Level::Warning, message);
}

pub fn info(message: &str) {
    write(Level::Info, message);
}

pub fn debug(message: &str) {
    write(Level::Debug, message);
}

fn write(level: Level, message: &str) {
    let mut state = lock_state();
    if level == Level::Disabled || state.level < level {
        return;
    }
    let Some(path) = state.path.clone() else {
        return;
    };
    if fs::metadata(&path).is_ok_and(|metadata| metadata.len() > ONE_MEBIBYTE) {
        let old = path.with_file_name(format!(
            "{}.1",
            path.file_name().unwrap_or_default().to_string_lossy()
        ));
        let _ = fs::remove_file(&old);
        if fs::rename(&path, old).is_err() {
            return;
        }
        state.wrote_header = false;
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    if (!state.wrote_header || file.metadata().is_ok_and(|metadata| metadata.len() == 0))
        && writeln!(file, "\n===> btoprs v{}", env!("CARGO_PKG_VERSION")).is_ok()
    {
        state.wrote_header = true;
    }
    let name = match level {
        Level::Disabled => "DISABLED",
        Level::Error => "ERROR",
        Level::Warning => "WARNING",
        Level::Info => "INFO",
        Level::Debug => "DEBUG",
    };
    let _ = writeln!(file, "{}Z | {name}: {message}", utc_timestamp());
}

fn log_path() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .map(|path| path.join("btoprs/btop.log"))
    }
    #[cfg(unix)]
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|path| path.join("btop.log"))
}

#[cfg(unix)]
#[repr(C)]
struct Tm {
    sec: c_int,
    min: c_int,
    hour: c_int,
    mday: c_int,
    mon: c_int,
    year: c_int,
    wday: c_int,
    yday: c_int,
    isdst: c_int,
    gmtoff: c_long,
    zone: *const c_char,
}

#[cfg(unix)]
fn utc_timestamp() -> String {
    unsafe extern "C" {
        fn time(value: *mut TimeT) -> TimeT;
        fn gmtime_r(value: *const TimeT, result: *mut Tm) -> *mut Tm;
        fn strftime(
            output: *mut c_char,
            size: usize,
            format: *const c_char,
            value: *const Tm,
        ) -> usize;
    }
    let now = unsafe { time(std::ptr::null_mut()) };
    let mut utc = std::mem::MaybeUninit::<Tm>::uninit();
    if unsafe { gmtime_r(&now, utc.as_mut_ptr()) }.is_null() {
        return "1970-01-01T00:00:00".into();
    }
    let utc = unsafe { utc.assume_init() };
    let mut output = [0 as c_char; 32];
    let format = CString::new("%FT%T").unwrap();
    if unsafe { strftime(output.as_mut_ptr(), output.len(), format.as_ptr(), &utc) } == 0 {
        return "1970-01-01T00:00:00".into();
    }
    unsafe { CStr::from_ptr(output.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(windows)]
fn utc_timestamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    // Howard Hinnant's civil-from-days algorithm, with Unix day zero shifted
    // to the proleptic Gregorian epoch used by the formula.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}",
        day_seconds / 3_600,
        day_seconds / 60 % 60,
        day_seconds % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn utc_log_timestamp_has_reference_shape() {
        let timestamp = utc_timestamp();
        assert_eq!(timestamp.len(), 19);
        assert_eq!(&timestamp[4..5], "-");
        assert_eq!(&timestamp[10..11], "T");
    }

    #[test]
    fn rotated_log_starts_with_a_fresh_header() {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("btoprs-log-{}-{suffix}", std::process::id()));
        fs::write(&path, vec![b'x'; ONE_MEBIBYTE as usize + 1]).unwrap();

        let previous = {
            let mut state = lock_state();
            std::mem::replace(
                &mut *state,
                State {
                    level: Level::Info,
                    path: Some(path.clone()),
                    wrote_header: true,
                },
            )
        };
        write(Level::Info, "after rotation");
        let contents = fs::read_to_string(&path).unwrap();
        assert!(contents.contains("===> btoprs v"));
        assert!(contents.contains("INFO: after rotation"));
        let old_path =
            path.with_file_name(format!("{}.1", path.file_name().unwrap().to_string_lossy()));
        assert!(old_path.exists());

        *lock_state() = previous;
        let _ = fs::remove_file(old_path);
        let _ = fs::remove_file(path);
    }
}
