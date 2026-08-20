use std::ffi::{CStr, CString};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::os::raw::{c_char, c_int, c_long};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

const ONE_MEBIBYTE: u64 = 1024 * 1024;

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

pub fn init() {
    let Some(path) = log_path() else {
        return;
    };
    if let Some(parent) = path.parent()
        && fs::create_dir_all(parent).is_err()
    {
        return;
    }
    state().lock().unwrap().path = Some(path);
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
    state().lock().unwrap().level = level;
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
    let mut state = state().lock().unwrap();
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
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&path) else {
        return;
    };
    if !state.wrote_header {
        let _ = writeln!(file, "\n===> btop++ v{}", env!("CARGO_PKG_VERSION"));
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
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/state")))
        .map(|path| path.join("btop.log"))
}

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

fn utc_timestamp() -> String {
    unsafe extern "C" {
        fn time(value: *mut c_long) -> c_long;
        fn gmtime_r(value: *const c_long, result: *mut Tm) -> *mut Tm;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_log_timestamp_has_reference_shape() {
        let timestamp = utc_timestamp();
        assert_eq!(timestamp.len(), 19);
        assert_eq!(&timestamp[4..5], "-");
        assert_eq!(&timestamp[10..11], "T");
    }
}
