pub fn bytes(value: u64, base_10: bool) -> String {
    human_bytes(value, base_10, false)
}

pub fn bytes_spaced(value: u64, base_10: bool) -> String {
    bytes(value, base_10)
}

pub fn bytes_short(value: u64, base_10: bool) -> String {
    human_bytes(value, base_10, true)
}

fn human_bytes(value: u64, base_10: bool, shorten: bool) -> String {
    let units = if base_10 {
        ["Byte", "kB", "MB", "GB", "TB", "PB", "EB"]
    } else {
        ["Byte", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"]
    };
    let base: u128 = if base_10 { 1000 } else { 1024 };
    let mut value = u128::from(value) * 100;
    let mut unit = 0;
    while value >= base * 100 && unit + 1 < units.len() {
        value /= base;
        unit += 1;
    }
    let mut out = value.to_string();
    if !base_10 && out.len() == 4 && unit > 0 {
        out.pop();
        out.insert(2, '.');
    } else if out.len() == 3 && unit > 0 {
        out.insert(1, '.');
    } else if out.len() >= 2 {
        out.truncate(out.len() - 2);
    }
    if out.is_empty() {
        out.push('0');
    }
    if shorten {
        let had_decimal = out.contains('.');
        if had_decimal {
            out = format!("{:.1}", out.parse::<f64>().unwrap_or(0.0));
        }
        if out.len() > 3 {
            if had_decimal {
                out = format!("{:.0}", out.parse::<f64>().unwrap_or(0.0));
            } else {
                out = format!("{}.0", out.as_bytes()[0] - b'0');
                unit = (unit + 1).min(units.len() - 1);
            }
        }
        out.push(units[unit].chars().next().unwrap_or('B'));
    } else {
        out.push(' ');
        out.push_str(units[unit]);
    }
    out
}

pub fn bits_per_second(bytes_per_second: u64, base_10: bool) -> String {
    let units = if base_10 {
        ["bit", "kb", "Mb", "Gb", "Tb", "Pb", "Eb"]
    } else {
        ["bit", "Kib", "Mib", "Gib", "Tib", "Pib", "Eib"]
    };
    let base: u128 = if base_10 { 1000 } else { 1024 };
    let mut value = u128::from(bytes_per_second) * 800;
    let mut unit = 0;
    while value >= base * 100 && unit < units.len() - 1 {
        value /= base;
        unit += 1;
    }

    let mut out = value.to_string();
    if !base_10 && out.len() == 4 && unit > 0 {
        out.pop();
        out.insert(2, '.');
    } else if out.len() == 3 && unit > 0 {
        out.insert(1, '.');
    } else if out.len() >= 2 {
        out.truncate(out.len() - 2);
    }
    if out.is_empty() {
        out.push('0');
    }
    format!("{out} {}ps", units[unit])
}

pub fn duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else {
        format!("{hours:02}:{minutes:02}:{:02}", seconds % 60)
    }
}

pub fn truncate(text: &str, width: usize) -> String {
    if display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    if width == 1 {
        return "…".into();
    }
    let target = width - 1;
    let mut used = 0;
    let mut output = String::new();
    for ch in text.chars() {
        let char_width = char_width(ch);
        if used + char_width > target {
            break;
        }
        output.push(ch);
        used += char_width;
    }
    output.push('…');
    output
}

pub fn display_width(text: &str) -> usize {
    text.chars().map(char_width).sum()
}

pub fn column_slice(text: &str, start: usize, width: usize) -> String {
    let mut position = 0;
    let mut used = 0;
    let mut output = String::new();
    for ch in text.chars() {
        let ch_width = char_width(ch);
        if ch_width == 0 {
            if !output.is_empty() {
                output.push(ch);
            }
            continue;
        }
        if position + ch_width <= start {
            position += ch_width;
            continue;
        }
        if position < start {
            position += ch_width;
            continue;
        }
        if used + ch_width > width {
            break;
        }
        output.push(ch);
        position += ch_width;
        used += ch_width;
    }
    output
}

pub fn char_width(ch: char) -> usize {
    unsafe extern "C" {
        fn wcwidth(ch: i32) -> i32;
    }
    let width = unsafe { wcwidth(ch as i32) };
    if width >= 0 {
        width as usize
    } else if matches!(ch as u32, 0x0300..=0x036f | 0x1ab0..=0x1aff | 0x1dc0..=0x1dff | 0x20d0..=0x20ff | 0xfe00..=0xfe0f | 0xfe20..=0xfe2f | 0xe0100..=0xe01ef)
    {
        0
    } else if matches!(ch as u32, 0x1100..=0x115f | 0x2329..=0x232a | 0x2e80..=0xa4cf | 0xac00..=0xd7a3 | 0xf900..=0xfaff | 0xfe10..=0xfe19 | 0xfe30..=0xfe6f | 0xff00..=0xff60 | 0xffe0..=0xffe6 | 0x1f300..=0x1faff | 0x20000..=0x3fffd)
    {
        2
    } else if ch.is_control() {
        0
    } else {
        1
    }
}

#[cfg(unix)]
pub fn local_clock_format(format: &str) -> String {
    use std::ffi::CStr;
    use std::os::raw::{c_char, c_int, c_long};

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
    unsafe extern "C" {
        fn time(value: *mut c_long) -> c_long;
        fn localtime_r(value: *const c_long, result: *mut Tm) -> *mut Tm;
        fn strftime(
            output: *mut c_char,
            size: usize,
            format: *const c_char,
            value: *const Tm,
        ) -> usize;
    }
    let now = unsafe { time(std::ptr::null_mut()) };
    let mut local = std::mem::MaybeUninit::<Tm>::uninit();
    if unsafe { localtime_r(&now, local.as_mut_ptr()) }.is_null() {
        return String::new();
    }
    let local = unsafe { local.assume_init() };
    let mut output = [0 as c_char; 64];
    let Ok(format) = std::ffi::CString::new(format) else {
        return String::new();
    };
    if unsafe { strftime(output.as_mut_ptr(), output.len(), format.as_ptr(), &local) } == 0 {
        return String::new();
    }
    unsafe { CStr::from_ptr(output.as_ptr()) }
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_btop_integer_humanizer() {
        assert_eq!(bytes(1_127_219_200, false), "1.04 GiB");
        assert_eq!(bytes_short(1_048_576, false), "1.0M");
        assert_eq!(bytes_short(15 * 1_048_576, false), "15M");
        assert_eq!(bits_per_second(1_024, false), "8.00 Kibps");
        assert_eq!(bits_per_second(1_000, true), "8.00 kbps");
        assert_eq!(bits_per_second(36 * 1_024, false), "288 Kibps");
        assert_eq!(bits_per_second(1_875_000, false), "14.3 Mibps");
    }

    #[test]
    fn humanizers_do_not_overflow_at_u64_scale() {
        assert_eq!(bytes(u64::MAX, false), "15.9 EiB");
        assert_eq!(bytes(u64::MAX, true), "18 EB");
        assert_eq!(bytes_short(u64::MAX, false), "16E");
        assert_eq!(bits_per_second(u64::MAX, false), "127 Eibps");
    }

    #[test]
    fn truncates_by_terminal_columns() {
        assert_eq!(display_width("A界B"), 4);
        assert_eq!(truncate("A界B", 3), "A…");
        assert_eq!(display_width("e\u{301}"), 1);
        assert_eq!(column_slice("ab界cd", 2, 2), "界");
        assert_eq!(column_slice("ab界cd", 4, 2), "cd");
        assert_eq!(column_slice("e\u{301}x", 0, 1), "e\u{301}");
    }

    #[test]
    fn clock_uses_the_configured_strftime_format() {
        let year = local_clock_format("%Y");
        assert_eq!(year.len(), 4);
        assert!(year.chars().all(|ch| ch.is_ascii_digit()));
        assert_eq!(local_clock_format("literal"), "literal");
    }
}

#[cfg(not(unix))]
pub fn local_clock_format(_format: &str) -> String {
    String::new()
}
