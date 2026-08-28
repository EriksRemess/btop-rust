use std::cell::UnsafeCell;
use std::io::{self, Read, Write};
use std::mem::MaybeUninit;
use std::os::raw::{c_int, c_short, c_uchar, c_uint, c_ulong};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

const STDIN: c_int = 0;
const STDOUT: c_int = 1;
const TCSANOW: c_int = 0;
#[cfg(target_os = "linux")]
const ICANON: c_uint = 0x0000_0002;
#[cfg(target_os = "macos")]
const ICANON: c_ulong = 0x0000_0100;
#[cfg(target_os = "linux")]
const ECHO: c_uint = 0x0000_0008;
#[cfg(target_os = "macos")]
const ECHO: c_ulong = 0x0000_0008;
#[cfg(target_os = "linux")]
const ISIG: c_uint = 0x0000_0001;
#[cfg(target_os = "macos")]
const ISIG: c_ulong = 0x0000_0080;
#[cfg(target_os = "linux")]
const IXON: c_uint = 0x0000_0400;
#[cfg(target_os = "macos")]
const IXON: c_ulong = 0x0000_0200;
#[cfg(target_os = "linux")]
const ICRNL: c_uint = 0x0000_0100;
#[cfg(target_os = "macos")]
const ICRNL: c_ulong = 0x0000_0100;
#[cfg(target_os = "linux")]
const OPOST: c_uint = 0x0000_0001;
#[cfg(target_os = "macos")]
const OPOST: c_ulong = 0x0000_0001;
#[cfg(target_os = "linux")]
const VTIME: usize = 5;
#[cfg(target_os = "macos")]
const VTIME: usize = 17;
#[cfg(target_os = "linux")]
const VMIN: usize = 6;
#[cfg(target_os = "macos")]
const VMIN: usize = 16;
#[cfg(target_os = "linux")]
const TIOCGWINSZ: c_ulong = 0x5413;
#[cfg(target_os = "macos")]
const TIOCGWINSZ: c_ulong = 0x4008_7468;
const POLLIN: c_short = 0x0001;
const F_GETFL: c_int = 3;
const F_SETFL: c_int = 4;
#[cfg(target_os = "linux")]
const O_NONBLOCK: c_int = 0x800;
#[cfg(target_os = "macos")]
const O_NONBLOCK: c_int = 0x0000_0004;
const ESCAPE_SEQUENCE_TIMEOUT: Duration = Duration::from_millis(25);

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_os = "linux")]
struct Termios {
    c_iflag: c_uint,
    c_oflag: c_uint,
    c_cflag: c_uint,
    c_lflag: c_uint,
    c_line: c_uchar,
    c_cc: [c_uchar; 32],
    c_ispeed: c_uint,
    c_ospeed: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy)]
#[cfg(target_os = "macos")]
struct Termios {
    c_iflag: c_ulong,
    c_oflag: c_ulong,
    c_cflag: c_ulong,
    c_lflag: c_ulong,
    c_cc: [c_uchar; 20],
    c_ispeed: c_ulong,
    c_ospeed: c_ulong,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct WinSize {
    rows: u16,
    cols: u16,
    xpixel: u16,
    ypixel: u16,
}

#[repr(C)]
struct PollFd {
    fd: c_int,
    events: c_short,
    revents: c_short,
}

unsafe extern "C" {
    fn tcgetattr(fd: c_int, termios: *mut Termios) -> c_int;
    fn tcsetattr(fd: c_int, action: c_int, termios: *const Termios) -> c_int;
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    #[cfg(target_os = "linux")]
    fn poll(fds: *mut PollFd, count: c_ulong, timeout: c_int) -> c_int;
    #[cfg(target_os = "macos")]
    fn poll(fds: *mut PollFd, count: c_uint, timeout: c_int) -> c_int;
    fn write(fd: c_int, buffer: *const u8, count: usize) -> isize;
    fn fcntl(fd: c_int, command: c_int, ...) -> c_int;
}

struct CrashTermios(UnsafeCell<MaybeUninit<Termios>>);

// The crash handler is the only reader, and signals cannot run concurrently on
// the same thread. CRASH_TERMIOS_READY publishes the completed copy.
unsafe impl Sync for CrashTermios {}

static CRASH_TERMIOS: CrashTermios = CrashTermios(UnsafeCell::new(MaybeUninit::uninit()));
static CRASH_TERMIOS_READY: AtomicBool = AtomicBool::new(false);
const CRASH_RESTORE_SEQUENCE: &[u8] =
    b"\x1b[?2026l\x1b[?1002l\x1b[?1015l\x1b[?1006l\x1b[?7h\x1b[?25h\x1b[?1049l\x1b[0m";

/// Restore the terminal using only libc calls that are safe to make from the
/// fatal-signal path. The handler re-raises the signal after calling this.
pub unsafe fn restore_after_crash() {
    if CRASH_TERMIOS_READY.swap(false, Ordering::SeqCst) {
        // SAFETY: enter() initialized the published value before setting the
        // ready flag, and the flag prevents a second restoration attempt.
        let original = unsafe { (*CRASH_TERMIOS.0.get()).assume_init_ref() };
        unsafe {
            tcsetattr(STDIN, TCSANOW, original);
        }
    }
    // A full terminal/PTY must not keep a fatal handler from reaching the
    // default re-raise. Make this best-effort write nonblocking.
    unsafe {
        let flags = fcntl(STDOUT, F_GETFL);
        if flags >= 0 {
            fcntl(STDOUT, F_SETFL, flags | O_NONBLOCK);
        }
        write(
            STDOUT,
            CRASH_RESTORE_SEQUENCE.as_ptr(),
            CRASH_RESTORE_SEQUENCE.len(),
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Size {
    pub cols: u16,
    pub rows: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    Char(char),
    Up,
    Down,
    Left,
    Right,
    PageUp,
    PageDown,
    Home,
    End,
    Enter,
    Escape,
    Backspace,
    Delete,
    Insert,
    Tab,
    BackTab,
    CtrlC,
    CtrlR,
    CtrlZ,
    F1,
    F2,
    Function(u8),
    Mouse {
        button: u16,
        x: u16,
        y: u16,
        pressed: bool,
    },
    Unknown,
}

pub struct Terminal {
    original: Termios,
    active: bool,
    pending: Vec<u8>,
    mouse_enabled: bool,
    synchronized: bool,
}

impl Terminal {
    pub fn enter(mouse_enabled: bool, synchronized: bool) -> Result<Self, String> {
        let mut original = MaybeUninit::<Termios>::uninit();
        if unsafe { tcgetattr(STDIN, original.as_mut_ptr()) } != 0 {
            return Err(format!(
                "could not read terminal settings: {}",
                io::Error::last_os_error()
            ));
        }
        let original = unsafe { original.assume_init() };
        let mut raw = original;
        raw.c_iflag &= !(IXON | ICRNL);
        raw.c_oflag |= OPOST;
        raw.c_lflag &= !(ICANON | ECHO | ISIG);
        raw.c_cc[VMIN] = 0;
        raw.c_cc[VTIME] = 0;
        if unsafe { tcsetattr(STDIN, TCSANOW, &raw) } != 0 {
            return Err(format!(
                "could not enable raw terminal mode: {}",
                io::Error::last_os_error()
            ));
        }
        // SAFETY: the value is fully copied before the release store publishes
        // it to the fatal-signal handler.
        unsafe {
            (*CRASH_TERMIOS.0.get()).write(original);
        }
        CRASH_TERMIOS_READY.store(true, Ordering::SeqCst);
        let mut terminal = Self {
            original,
            active: true,
            pending: Vec::new(),
            mouse_enabled,
            synchronized,
        };
        let output_result = (|| -> io::Result<()> {
            let mut stdout = io::stdout().lock();
            stdout.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?7l")?;
            if mouse_enabled {
                stdout.write_all(b"\x1b[?1002h\x1b[?1015h\x1b[?1006h")?;
            }
            stdout.write_all(b"\x1b[2J\x1b[H")?;
            stdout.flush()
        })();
        if let Err(error) = output_result {
            // Restore both the display modes and termios. If cleanup itself
            // fails, Drop retains an active Terminal and makes another pass.
            return match terminal.leave() {
                Ok(()) => Err(error.to_string()),
                Err(cleanup) => Err(format!("{error}; terminal cleanup also failed: {cleanup}")),
            };
        }
        Ok(terminal)
    }

    pub fn size(&self) -> Result<Size, String> {
        let mut size = WinSize::default();
        if unsafe { ioctl(STDOUT, TIOCGWINSZ, &mut size) } != 0 {
            return Err(format!(
                "could not get terminal size: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(Size {
            cols: size.cols,
            rows: size.rows,
        })
    }

    pub fn draw(&mut self, frame: &str) -> Result<(), String> {
        let mut stdout = io::stdout().lock();
        let result = (|| -> io::Result<()> {
            if self.synchronized {
                stdout.write_all(b"\x1b[?2026h")?;
            }
            stdout.write_all(b"\x1b[H")?;
            stdout.write_all(frame.as_bytes())?;
            if self.synchronized {
                stdout.write_all(b"\x1b[?2026l")?;
            }
            stdout.flush()
        })();
        if result.is_err() && self.synchronized {
            // Do not knowingly leave a terminal in a synchronized-update
            // transaction if a frame write fails part-way through.
            let _ = stdout.write_all(b"\x1b[?2026l");
            let _ = stdout.flush();
        }
        result.map_err(|error| error.to_string())
    }

    pub fn apply_settings(
        &mut self,
        mouse_enabled: bool,
        synchronized: bool,
    ) -> Result<(), String> {
        if self.mouse_enabled != mouse_enabled {
            let sequence = if mouse_enabled {
                // Record the conservative state before writing: even a failed
                // write may have enabled one or more mouse protocols, and
                // leave() must disable them again.
                self.mouse_enabled = true;
                b"\x1b[?1002h\x1b[?1015h\x1b[?1006h".as_slice()
            } else {
                b"\x1b[?1002l\x1b[?1015l\x1b[?1006l".as_slice()
            };
            let mut stdout = io::stdout().lock();
            stdout
                .write_all(sequence)
                .and_then(|()| stdout.flush())
                .map_err(|error| error.to_string())?;
            self.mouse_enabled = mouse_enabled;
        }
        self.synchronized = synchronized;
        Ok(())
    }

    pub fn read_key(&mut self, timeout: Duration) -> Result<Option<Key>, String> {
        if let Some(key) = take_key(&mut self.pending) {
            return Ok(Some(key));
        }
        let waiting_for_sequence = !self.pending.is_empty();
        let mut fd = PollFd {
            fd: STDIN,
            events: POLLIN,
            revents: 0,
        };
        let poll_timeout = if waiting_for_sequence {
            timeout.min(ESCAPE_SEQUENCE_TIMEOUT)
        } else {
            timeout
        };
        let millis = poll_timeout.as_millis().min(c_int::MAX as u128) as c_int;
        let ready = unsafe { poll(&mut fd, 1, millis) };
        if ready < 0 {
            if io::Error::last_os_error().raw_os_error() == Some(4) {
                return Ok(None);
            }
            return Err(format!(
                "input polling failed: {}",
                io::Error::last_os_error()
            ));
        }
        if ready == 0 {
            return Ok(if waiting_for_sequence {
                take_key_after_sequence_timeout(&mut self.pending)
            } else {
                None
            });
        }
        // A terminal writes mouse reports as escape sequences. Drain everything
        // currently available, as btop does, so a busy wheel cannot leave a
        // report split at an arbitrary fixed-size read boundary.
        let mut stdin = io::stdin().lock();
        let mut bytes = [0u8; 1024];
        loop {
            let count = stdin.read(&mut bytes).map_err(|e| e.to_string())?;
            if count == 0 {
                break;
            }
            self.pending.extend_from_slice(&bytes[..count]);
        }
        Ok(take_key(&mut self.pending))
    }

    pub fn leave(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        let output_result = (|| -> io::Result<()> {
            let mut stdout = io::stdout().lock();
            if self.synchronized {
                stdout.write_all(b"\x1b[?2026l")?;
            }
            if self.mouse_enabled {
                stdout.write_all(b"\x1b[?1002l\x1b[?1015l\x1b[?1006l")?;
            }
            stdout.write_all(b"\x1b[?7h\x1b[?25h\x1b[?1049l\x1b[0m")?;
            stdout.flush()
        })();
        let terminal_result = if unsafe { tcsetattr(STDIN, TCSANOW, &self.original) } == 0 {
            Ok(())
        } else {
            Err(format!(
                "could not restore terminal settings: {}",
                io::Error::last_os_error()
            ))
        };
        terminal_result?;
        CRASH_TERMIOS_READY.store(false, Ordering::SeqCst);
        self.active = false;
        output_result.map_err(|error| error.to_string())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

fn take_key(bytes: &mut Vec<u8>) -> Option<Key> {
    let mut key = take_key_inner(bytes, false)?;
    if is_mouse_wheel(key) {
        // btop drains a whole input burst before interpreting it, which turns a
        // run of wheel reports into one scroll action. Do the same without
        // discarding a keyboard or click event queued immediately afterwards.
        while let Some((next, consumed)) = parse_sgr_mouse(bytes) {
            if !is_mouse_wheel(next) {
                break;
            }
            bytes.drain(..consumed);
            key = next;
        }
    }
    Some(key)
}

fn take_key_after_sequence_timeout(bytes: &mut Vec<u8>) -> Option<Key> {
    take_key_inner(bytes, true)
}

fn take_key_inner(bytes: &mut Vec<u8>, sequence_timed_out: bool) -> Option<Key> {
    if escape_sequence_is_incomplete(bytes) {
        if !sequence_timed_out {
            return None;
        }
        if bytes.as_slice() == [0x1b] {
            bytes.clear();
            return Some(Key::Escape);
        }
        // Never reinterpret a truncated terminal control sequence as Escape:
        // Escape opens the main menu, which made burst mouse input flicker it.
        bytes.clear();
        return Some(Key::Unknown);
    }
    let (key, consumed) = match bytes.as_slice() {
        [] => return None,
        [3, ..] => (Key::CtrlC, 1),
        [18, ..] => (Key::CtrlR, 1),
        [26, ..] => (Key::CtrlZ, 1),
        [b'\r' | b'\n', ..] => (Key::Enter, 1),
        [127 | 8, ..] => (Key::Backspace, 1),
        [b'\t', ..] => (Key::Tab, 1),
        [0x1b, b'[', b'A', ..] | [0x1b, b'O', b'A', ..] => (Key::Up, 3),
        [0x1b, b'[', b'B', ..] | [0x1b, b'O', b'B', ..] => (Key::Down, 3),
        [0x1b, b'[', b'C', ..] | [0x1b, b'O', b'C', ..] => (Key::Right, 3),
        [0x1b, b'[', b'D', ..] | [0x1b, b'O', b'D', ..] => (Key::Left, 3),
        [0x1b, b'[', b'H', ..] | [0x1b, b'O', b'H', ..] => (Key::Home, 3),
        [0x1b, b'[', b'1', b'~', ..] => (Key::Home, 4),
        [0x1b, b'[', b'F', ..] | [0x1b, b'O', b'F', ..] => (Key::End, 3),
        [0x1b, b'[', b'4', b'~', ..] => (Key::End, 4),
        [0x1b, b'[', b'5', b'~', ..] => (Key::PageUp, 4),
        [0x1b, b'[', b'6', b'~', ..] => (Key::PageDown, 4),
        [0x1b, b'[', b'2', b'~', ..] | [0x1b, b'[', b'4', b'h', ..] => (Key::Insert, 4),
        [0x1b, b'[', b'3', b'~', ..] => (Key::Delete, 4),
        [0x1b, b'[', b'P', ..] => (Key::Delete, 3),
        [0x1b, b'[', b'Z', ..] => (Key::BackTab, 3),
        [0x1b, b'O', b'P', ..] => (Key::F1, 3),
        [0x1b, b'O', b'Q', ..] => (Key::F2, 3),
        [0x1b, b'O', b'R', ..] => (Key::Function(3), 3),
        [0x1b, b'O', b'S', ..] => (Key::Function(4), 3),
        [0x1b, b'[', b'1', b'1', b'~', ..] => (Key::F1, 5),
        [0x1b, b'[', b'1', b'2', b'~', ..] => (Key::F2, 5),
        [0x1b, b'[', b'1', b'5', b'~', ..] => (Key::Function(5), 5),
        [0x1b, b'[', b'1', b'7', b'~', ..] => (Key::Function(6), 5),
        [0x1b, b'[', b'1', b'8', b'~', ..] => (Key::Function(7), 5),
        [0x1b, b'[', b'1', b'9', b'~', ..] => (Key::Function(8), 5),
        [0x1b, b'[', b'2', b'0', b'~', ..] => (Key::Function(9), 5),
        [0x1b, b'[', b'2', b'1', b'~', ..] => (Key::Function(10), 5),
        [0x1b, b'[', b'2', b'3', b'~', ..] => (Key::Function(11), 5),
        [0x1b, b'[', b'2', b'4', b'~', ..] => (Key::Function(12), 5),
        [0x1b, b'[', b'<', ..] => parse_sgr_mouse(bytes)?,
        [0x1b, b'[', ..] | [0x1b, b'O', ..] => (
            Key::Unknown,
            escape_sequence_length(bytes).unwrap_or(bytes.len()),
        ),
        [0x1b, ..] => (Key::Escape, 1),
        [byte, ..] if byte.is_ascii() && !byte.is_ascii_control() => (Key::Char(*byte as char), 1),
        [byte, ..] if *byte >= 0x80 => {
            let width = utf8_sequence_width(*byte);
            if width == 0 {
                (Key::Unknown, 1)
            } else if bytes.len() < width {
                return None;
            } else if let Ok(text) = std::str::from_utf8(&bytes[..width]) {
                (Key::Char(text.chars().next()?), width)
            } else {
                (Key::Unknown, 1)
            }
        }
        _ => (Key::Unknown, 1),
    };
    bytes.drain(..consumed);
    Some(key)
}

fn parse_sgr_mouse(bytes: &[u8]) -> Option<(Key, usize)> {
    let [0x1b, b'[', b'<', rest @ ..] = bytes else {
        return None;
    };
    let end = rest.iter().position(|byte| matches!(byte, b'M' | b'm'))?;
    let pressed = rest[end] == b'M';
    let fields = std::str::from_utf8(&rest[..end]).ok().and_then(|text| {
        text.split(';')
            .map(str::parse::<u16>)
            .collect::<Result<Vec<_>, _>>()
            .ok()
    });
    let key = match fields.as_deref() {
        Some([button, x, y]) if *x > 0 && *y > 0 => Key::Mouse {
            button: *button,
            x: x - 1,
            y: y - 1,
            pressed,
        },
        _ => Key::Unknown,
    };
    Some((key, end + 4))
}

fn is_mouse_wheel(key: Key) -> bool {
    matches!(
        key,
        Key::Mouse {
            button: 64 | 65,
            ..
        }
    )
}

fn escape_sequence_is_incomplete(bytes: &[u8]) -> bool {
    match bytes {
        [0x1b] => true,
        [0x1b, b'[', ..] | [0x1b, b'O', ..] => escape_sequence_length(bytes).is_none(),
        _ => false,
    }
}

fn escape_sequence_length(bytes: &[u8]) -> Option<usize> {
    match bytes {
        [0x1b, b'[', b'<', rest @ ..] => rest
            .iter()
            .position(|byte| matches!(byte, b'M' | b'm'))
            .map(|end| end + 4),
        [0x1b, b'[', rest @ ..] => rest
            .iter()
            .position(|byte| (0x40..=0x7e).contains(byte))
            .map(|end| end + 3),
        [0x1b, b'O', rest @ ..] => rest.first().map(|_| 3),
        _ => None,
    }
}

fn utf8_sequence_width(first: u8) -> usize {
    match first {
        0xC2..=0xDF => 2,
        0xE0..=0xEF => 3,
        0xF0..=0xF4 => 4,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(target_os = "macos")]
    #[test]
    fn darwin_terminal_abi_matches_system_headers() {
        assert_eq!(std::mem::size_of::<Termios>(), 72);
        assert_eq!(std::mem::size_of::<WinSize>(), 8);
        assert_eq!(TIOCGWINSZ, 0x4008_7468);
        assert_eq!((VMIN, VTIME), (16, 17));
    }

    #[test]
    fn preserves_coalesced_keys() {
        let mut bytes = b"e\x1b[B\r?q".to_vec();
        assert_eq!(take_key(&mut bytes), Some(Key::Char('e')));
        assert_eq!(take_key(&mut bytes), Some(Key::Down));
        assert_eq!(take_key(&mut bytes), Some(Key::Enter));
        assert_eq!(take_key(&mut bytes), Some(Key::Char('?')));
        assert_eq!(take_key(&mut bytes), Some(Key::Char('q')));
        assert!(bytes.is_empty());
    }

    #[test]
    fn parses_sgr_mouse_events() {
        let mut bytes = b"\x1b[<0;12;7M".to_vec();
        assert_eq!(
            take_key(&mut bytes),
            Some(Key::Mouse {
                button: 0,
                x: 11,
                y: 6,
                pressed: true,
            })
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn malformed_mouse_fields_are_consumed_without_becoming_clicks() {
        for report in [
            b"\x1b[<0;bad;7M".as_slice(),
            b"\x1b[<0;12;bad;7M".as_slice(),
            b"\x1b[<0;0;7M".as_slice(),
            b"\x1b[<0;12;0M".as_slice(),
        ] {
            let mut bytes = report.to_vec();
            assert_eq!(take_key(&mut bytes), Some(Key::Unknown));
            assert!(bytes.is_empty());
        }
    }

    #[test]
    fn invalid_utf8_cannot_block_later_keyboard_input() {
        let mut bytes = vec![0xff, 0xc2, b' ', b'q'];
        assert_eq!(take_key(&mut bytes), Some(Key::Unknown));
        assert_eq!(take_key(&mut bytes), Some(Key::Unknown));
        assert_eq!(take_key(&mut bytes), Some(Key::Char(' ')));
        assert_eq!(take_key(&mut bytes), Some(Key::Char('q')));
        assert!(bytes.is_empty());

        let mut partial = vec![0xe2, 0x82];
        assert_eq!(take_key(&mut partial), None);
        partial.push(0xac);
        assert_eq!(take_key(&mut partial), Some(Key::Char('€')));
    }

    #[test]
    fn buffers_every_partial_sgr_mouse_report_instead_of_emitting_escape() {
        let report = b"\x1b[<65;123;47M";
        for split in 1..report.len() {
            let mut bytes = report[..split].to_vec();
            assert_eq!(
                take_key(&mut bytes),
                None,
                "split at byte {split} must remain buffered"
            );
            bytes.extend_from_slice(&report[split..]);
            assert_eq!(
                take_key(&mut bytes),
                Some(Key::Mouse {
                    button: 65,
                    x: 122,
                    y: 46,
                    pressed: true,
                })
            );
            assert!(bytes.is_empty());
        }
    }

    #[test]
    fn burst_mouse_reports_split_at_the_old_read_boundary_are_coalesced() {
        let burst = b"\x1b[<65;90;40M\x1b[<65;90;40M\x1b[<64;90;40M";
        let mut bytes = burst[..32].to_vec();
        let mut parsed = Vec::new();
        while let Some(key) = take_key(&mut bytes) {
            parsed.push(key);
        }
        assert!(!parsed.contains(&Key::Escape));
        assert!(!bytes.is_empty(), "the split report stays buffered");

        bytes.extend_from_slice(&burst[32..]);
        while let Some(key) = take_key(&mut bytes) {
            parsed.push(key);
        }
        assert_eq!(
            parsed,
            vec![
                Key::Mouse {
                    button: 65,
                    x: 89,
                    y: 39,
                    pressed: true,
                },
                Key::Mouse {
                    button: 64,
                    x: 89,
                    y: 39,
                    pressed: true,
                },
            ]
        );
        assert!(bytes.is_empty());
    }

    #[test]
    fn a_large_wheel_burst_cannot_starve_a_following_key() {
        let mut bytes = b"\x1b[<65;90;40M".repeat(500);
        bytes.push(b'q');
        assert_eq!(
            take_key(&mut bytes),
            Some(Key::Mouse {
                button: 65,
                x: 89,
                y: 39,
                pressed: true,
            })
        );
        assert_eq!(take_key(&mut bytes), Some(Key::Char('q')));
        assert!(bytes.is_empty());
    }

    #[test]
    fn only_a_lone_escape_becomes_escape_after_the_sequence_timeout() {
        let mut escape = vec![0x1b];
        assert_eq!(take_key(&mut escape), None);
        assert_eq!(
            take_key_after_sequence_timeout(&mut escape),
            Some(Key::Escape)
        );
        assert!(escape.is_empty());

        let mut truncated_mouse = b"\x1b[<65;90".to_vec();
        assert_eq!(take_key(&mut truncated_mouse), None);
        assert_eq!(
            take_key_after_sequence_timeout(&mut truncated_mouse),
            Some(Key::Unknown)
        );
        assert!(truncated_mouse.is_empty());
    }

    #[test]
    fn parses_menu_tab_navigation() {
        let mut bytes = b"\t\x1b[Z".to_vec();
        assert_eq!(take_key(&mut bytes), Some(Key::Tab));
        assert_eq!(take_key(&mut bytes), Some(Key::BackTab));
        assert!(bytes.is_empty());
    }

    #[test]
    fn parses_help_and_options_function_keys() {
        let mut bytes = b"\x1bOP\x1b[12~".to_vec();
        assert_eq!(take_key(&mut bytes), Some(Key::F1));
        assert_eq!(take_key(&mut bytes), Some(Key::F2));
        assert!(bytes.is_empty());
    }

    #[test]
    fn parses_all_reference_escape_variants_and_utf8_input() {
        let mut bytes =
            b"\x1bOA\x1bOB\x1bOC\x1bOD\x1b[1~\x1b[4~\x1b[2~\x1b[P\x1bOR\x1b[24~\xc4\x81".to_vec();
        for expected in [
            Key::Up,
            Key::Down,
            Key::Right,
            Key::Left,
            Key::Home,
            Key::End,
            Key::Insert,
            Key::Delete,
            Key::Function(3),
            Key::Function(12),
            Key::Char('ā'),
        ] {
            assert_eq!(take_key(&mut bytes), Some(expected));
        }
        assert!(bytes.is_empty());
    }
}
