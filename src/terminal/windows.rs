use std::io::{self, Write};
use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::time::Duration;

type Handle = isize;
const STD_INPUT_HANDLE: u32 = (-10i32) as u32;
const STD_OUTPUT_HANDLE: u32 = (-11i32) as u32;
const INVALID_HANDLE_VALUE: Handle = -1;
const ENABLE_PROCESSED_INPUT: u32 = 0x0001;
const ENABLE_LINE_INPUT: u32 = 0x0002;
const ENABLE_ECHO_INPUT: u32 = 0x0004;
const ENABLE_WINDOW_INPUT: u32 = 0x0008;
const ENABLE_MOUSE_INPUT: u32 = 0x0010;
const ENABLE_EXTENDED_FLAGS: u32 = 0x0080;
const ENABLE_QUICK_EDIT_MODE: u32 = 0x0040;
const ENABLE_PROCESSED_OUTPUT: u32 = 0x0001;
const ENABLE_VIRTUAL_TERMINAL_PROCESSING: u32 = 0x0004;
const KEY_EVENT: u16 = 0x0001;
const MOUSE_EVENT: u16 = 0x0002;
const WINDOW_BUFFER_SIZE_EVENT: u16 = 0x0004;
const WAIT_OBJECT_0: u32 = 0;
const WAIT_TIMEOUT: u32 = 258;
const LEFT_CTRL_PRESSED: u32 = 0x0008;
const RIGHT_CTRL_PRESSED: u32 = 0x0004;
const LEFT_ALT_PRESSED: u32 = 0x0002;
const RIGHT_ALT_PRESSED: u32 = 0x0001;
const SHIFT_PRESSED: u32 = 0x0010;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Coord {
    x: i16,
    y: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct SmallRect {
    left: i16,
    top: i16,
    right: i16,
    bottom: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ConsoleScreenBufferInfo {
    size: Coord,
    cursor_position: Coord,
    attributes: u16,
    window: SmallRect,
    maximum_window_size: Coord,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct KeyEventRecord {
    key_down: i32,
    repeat_count: u16,
    virtual_key_code: u16,
    virtual_scan_code: u16,
    unicode_char: u16,
    control_key_state: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MouseEventRecord {
    mouse_position: Coord,
    button_state: u32,
    control_key_state: u32,
    event_flags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
union InputEvent {
    key: KeyEventRecord,
    mouse: MouseEventRecord,
    words: [u32; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct InputRecord {
    event_type: u16,
    event: InputEvent,
}

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GetStdHandle(identifier: u32) -> Handle;
    fn GetConsoleMode(handle: Handle, mode: *mut u32) -> i32;
    fn SetConsoleMode(handle: Handle, mode: u32) -> i32;
    fn GetConsoleScreenBufferInfo(handle: Handle, info: *mut ConsoleScreenBufferInfo) -> i32;
    fn WaitForSingleObject(handle: Handle, milliseconds: u32) -> u32;
    fn ReadConsoleInputW(
        input: Handle,
        buffer: *mut InputRecord,
        length: u32,
        read: *mut u32,
    ) -> i32;
    fn SetConsoleOutputCP(code_page: u32) -> i32;
    fn GetConsoleOutputCP() -> u32;
}

static CRASH_INPUT: AtomicIsize = AtomicIsize::new(0);
static CRASH_OUTPUT: AtomicIsize = AtomicIsize::new(0);
static CRASH_INPUT_MODE: AtomicU32 = AtomicU32::new(0);
static CRASH_OUTPUT_MODE: AtomicU32 = AtomicU32::new(0);
static CRASH_OUTPUT_CODE_PAGE: AtomicU32 = AtomicU32::new(0);

pub unsafe fn restore_after_crash() {
    let input = CRASH_INPUT.swap(0, Ordering::SeqCst);
    let output = CRASH_OUTPUT.swap(0, Ordering::SeqCst);
    if input != 0 {
        unsafe { SetConsoleMode(input, CRASH_INPUT_MODE.load(Ordering::Relaxed)) };
    }
    if output != 0 {
        unsafe { SetConsoleMode(output, CRASH_OUTPUT_MODE.load(Ordering::Relaxed)) };
    }
    let code_page = CRASH_OUTPUT_CODE_PAGE.swap(0, Ordering::SeqCst);
    if code_page != 0 {
        unsafe { SetConsoleOutputCP(code_page) };
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
    input: Handle,
    output: Handle,
    original_input_mode: u32,
    original_output_mode: u32,
    original_output_code_page: u32,
    active: bool,
    mouse_enabled: bool,
    synchronized: bool,
    repeated_key: Option<(Key, u16)>,
    high_surrogate: Option<u16>,
}

impl Terminal {
    pub fn enter(mouse_enabled: bool, synchronized: bool) -> Result<Self, String> {
        let input = unsafe { GetStdHandle(STD_INPUT_HANDLE) };
        let output = unsafe { GetStdHandle(STD_OUTPUT_HANDLE) };
        if [input, output]
            .into_iter()
            .any(|handle| handle == 0 || handle == INVALID_HANDLE_VALUE)
        {
            return Err(format!(
                "could not open Windows console: {}",
                io::Error::last_os_error()
            ));
        }
        let mut original_input_mode = 0;
        let mut original_output_mode = 0;
        let original_output_code_page = unsafe { GetConsoleOutputCP() };
        if unsafe { GetConsoleMode(input, &mut original_input_mode) } == 0
            || unsafe { GetConsoleMode(output, &mut original_output_mode) } == 0
        {
            return Err(format!(
                "could not read Windows console mode: {}",
                io::Error::last_os_error()
            ));
        }
        let mut input_mode = original_input_mode
            & !(ENABLE_PROCESSED_INPUT
                | ENABLE_LINE_INPUT
                | ENABLE_ECHO_INPUT
                | ENABLE_QUICK_EDIT_MODE);
        input_mode |= ENABLE_WINDOW_INPUT | ENABLE_EXTENDED_FLAGS;
        if mouse_enabled {
            input_mode |= ENABLE_MOUSE_INPUT;
        } else {
            input_mode &= !ENABLE_MOUSE_INPUT;
        }
        let output_mode =
            original_output_mode | ENABLE_PROCESSED_OUTPUT | ENABLE_VIRTUAL_TERMINAL_PROCESSING;
        if unsafe { SetConsoleMode(input, input_mode) } == 0
            || unsafe { SetConsoleMode(output, output_mode) } == 0
        {
            let _ = unsafe { SetConsoleMode(input, original_input_mode) };
            let _ = unsafe { SetConsoleMode(output, original_output_mode) };
            return Err(format!(
                "could not enable Windows virtual terminal mode: {}",
                io::Error::last_os_error()
            ));
        }
        if unsafe { SetConsoleOutputCP(65001) } == 0 {
            let _ = unsafe { SetConsoleMode(input, original_input_mode) };
            let _ = unsafe { SetConsoleMode(output, original_output_mode) };
            return Err(format!(
                "could not enable UTF-8 console output: {}",
                io::Error::last_os_error()
            ));
        }
        CRASH_INPUT_MODE.store(original_input_mode, Ordering::Relaxed);
        CRASH_OUTPUT_MODE.store(original_output_mode, Ordering::Relaxed);
        CRASH_OUTPUT_CODE_PAGE.store(original_output_code_page, Ordering::Relaxed);
        CRASH_INPUT.store(input, Ordering::SeqCst);
        CRASH_OUTPUT.store(output, Ordering::SeqCst);
        let mut terminal = Self {
            input,
            output,
            original_input_mode,
            original_output_mode,
            original_output_code_page,
            active: true,
            mouse_enabled,
            synchronized,
            repeated_key: None,
            high_surrogate: None,
        };
        if let Err(error) = terminal.write_enter_sequence() {
            let _ = terminal.leave();
            return Err(error);
        }
        Ok(terminal)
    }

    fn write_enter_sequence(&self) -> Result<(), String> {
        let mut stdout = io::stdout().lock();
        stdout
            .write_all(b"\x1b[?1049h\x1b[?25l\x1b[?7l\x1b[2J\x1b[H")
            .and_then(|()| stdout.flush())
            .map_err(|error| error.to_string())
    }

    pub fn size(&self) -> Result<Size, String> {
        let mut info = ConsoleScreenBufferInfo::default();
        if unsafe { GetConsoleScreenBufferInfo(self.output, &mut info) } == 0 {
            return Err(format!(
                "could not get console size: {}",
                io::Error::last_os_error()
            ));
        }
        Ok(Size {
            cols: (info.window.right - info.window.left + 1).max(1) as u16,
            rows: (info.window.bottom - info.window.top + 1).max(1) as u16,
        })
    }

    pub fn draw(&mut self, frame: &str) -> Result<(), String> {
        let mut stdout = io::stdout().lock();
        if self.synchronized {
            stdout
                .write_all(b"\x1b[?2026h")
                .map_err(|e| e.to_string())?;
        }
        let result = stdout
            .write_all(b"\x1b[H")
            .and_then(|()| stdout.write_all(frame.as_bytes()));
        if self.synchronized {
            let _ = stdout.write_all(b"\x1b[?2026l");
        }
        result
            .and_then(|()| stdout.flush())
            .map_err(|error| error.to_string())
    }

    pub fn apply_settings(
        &mut self,
        mouse_enabled: bool,
        synchronized: bool,
    ) -> Result<(), String> {
        if mouse_enabled != self.mouse_enabled {
            let mut mode = 0;
            if unsafe { GetConsoleMode(self.input, &mut mode) } == 0 {
                return Err(io::Error::last_os_error().to_string());
            }
            if mouse_enabled {
                mode |= ENABLE_MOUSE_INPUT;
            } else {
                mode &= !ENABLE_MOUSE_INPUT;
            }
            if unsafe { SetConsoleMode(self.input, mode) } == 0 {
                return Err(io::Error::last_os_error().to_string());
            }
            self.mouse_enabled = mouse_enabled;
        }
        self.synchronized = synchronized;
        Ok(())
    }

    pub fn read_key(&mut self, timeout: Duration) -> Result<Option<Key>, String> {
        if let Some((key, remaining)) = self.repeated_key {
            self.repeated_key = (remaining > 1).then_some((key, remaining - 1));
            return Ok(Some(key));
        }
        let millis = timeout.as_millis().min(u32::MAX as u128) as u32;
        match unsafe { WaitForSingleObject(self.input, millis) } {
            WAIT_TIMEOUT => return Ok(None),
            WAIT_OBJECT_0 => {}
            _ => {
                return Err(format!(
                    "console input wait failed: {}",
                    io::Error::last_os_error()
                ));
            }
        }
        loop {
            let mut record = std::mem::MaybeUninit::<InputRecord>::uninit();
            let mut read = 0;
            if unsafe { ReadConsoleInputW(self.input, record.as_mut_ptr(), 1, &mut read) } == 0 {
                return Err(format!(
                    "console input read failed: {}",
                    io::Error::last_os_error()
                ));
            }
            if read == 0 {
                return Ok(None);
            }
            let record = unsafe { record.assume_init() };
            let (key, repeat_count) = match record.event_type {
                KEY_EVENT => {
                    let event = unsafe { record.event.key };
                    (
                        key_from_record(event, &mut self.high_surrogate),
                        event.repeat_count,
                    )
                }
                MOUSE_EVENT if self.mouse_enabled => {
                    (mouse_from_record(unsafe { record.event.mouse }), 1)
                }
                WINDOW_BUFFER_SIZE_EVENT => (Some(Key::Unknown), 1),
                _ => (None, 0),
            };
            if let Some(key) = key {
                self.repeated_key = (repeat_count > 1).then_some((key, repeat_count - 1));
                return Ok(Some(key));
            }
            if unsafe { WaitForSingleObject(self.input, 0) } != WAIT_OBJECT_0 {
                return Ok(None);
            }
        }
    }

    pub fn leave(&mut self) -> Result<(), String> {
        if !self.active {
            return Ok(());
        }
        let mut stdout = io::stdout().lock();
        let output_result = stdout
            .write_all(b"\x1b[?2026l\x1b[?7h\x1b[?25h\x1b[?1049l\x1b[0m")
            .and_then(|()| stdout.flush());
        let input_ok = unsafe { SetConsoleMode(self.input, self.original_input_mode) } != 0;
        let output_ok = unsafe { SetConsoleMode(self.output, self.original_output_mode) } != 0;
        let code_page_ok = self.original_output_code_page == 0
            || unsafe { SetConsoleOutputCP(self.original_output_code_page) } != 0;
        CRASH_INPUT.store(0, Ordering::SeqCst);
        CRASH_OUTPUT.store(0, Ordering::SeqCst);
        CRASH_OUTPUT_CODE_PAGE.store(0, Ordering::SeqCst);
        self.active = false;
        if !input_ok || !output_ok || !code_page_ok {
            return Err(format!(
                "could not restore Windows console mode: {}",
                io::Error::last_os_error()
            ));
        }
        output_result.map_err(|error| error.to_string())
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        let _ = self.leave();
    }
}

fn key_from_record(event: KeyEventRecord, high_surrogate: &mut Option<u16>) -> Option<Key> {
    if event.key_down == 0 {
        return None;
    }
    let ctrl = event.control_key_state & (LEFT_CTRL_PRESSED | RIGHT_CTRL_PRESSED) != 0;
    let alt = event.control_key_state & (LEFT_ALT_PRESSED | RIGHT_ALT_PRESSED) != 0;
    let alt_gr = ctrl && event.control_key_state & RIGHT_ALT_PRESSED != 0;
    if ctrl && !alt_gr {
        *high_surrogate = None;
        return match event.virtual_key_code {
            0x43 => Some(Key::CtrlC),
            0x52 => Some(Key::CtrlR),
            0x5a => Some(Key::CtrlZ),
            _ => Some(Key::Unknown),
        };
    }
    let key = match event.virtual_key_code {
        0x08 => Key::Backspace,
        0x09 if event.control_key_state & SHIFT_PRESSED != 0 => Key::BackTab,
        0x09 => Key::Tab,
        0x0d => Key::Enter,
        0x1b => Key::Escape,
        0x21 => Key::PageUp,
        0x22 => Key::PageDown,
        0x23 => Key::End,
        0x24 => Key::Home,
        0x25 => Key::Left,
        0x26 => Key::Up,
        0x27 => Key::Right,
        0x28 => Key::Down,
        0x2d => Key::Insert,
        0x2e => Key::Delete,
        0x70 => Key::F1,
        0x71 => Key::F2,
        0x72..=0x7b => Key::Function((event.virtual_key_code - 0x6f) as u8),
        _ => {
            let unit = event.unicode_char;
            if (0xd800..=0xdbff).contains(&unit) {
                *high_surrogate = Some(unit);
                return None;
            }
            let character = if (0xdc00..=0xdfff).contains(&unit) {
                let high = high_surrogate.take()?;
                char::decode_utf16([high, unit]).next()?.ok()?
            } else {
                *high_surrogate = None;
                char::from_u32(u32::from(unit))?
            };
            if character.is_control() || (alt && !alt_gr) {
                return None;
            }
            Key::Char(character)
        }
    };
    Some(key)
}

fn mouse_from_record(event: MouseEventRecord) -> Option<Key> {
    let x = event.mouse_position.x.max(0) as u16;
    let y = event.mouse_position.y.max(0) as u16;
    if event.event_flags == 0 {
        let button = if event.button_state & 1 != 0 {
            0
        } else if event.button_state & 4 != 0 {
            1
        } else if event.button_state & 2 != 0 {
            2
        } else {
            0
        };
        return Some(Key::Mouse {
            button,
            x,
            y,
            pressed: event.button_state != 0,
        });
    }
    if event.event_flags == 4 {
        let delta = (event.button_state >> 16) as i16;
        return Some(Key::Mouse {
            button: if delta > 0 { 64 } else { 65 },
            x,
            y,
            pressed: true,
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_console_abi_sizes_match_wincon_h() {
        assert_eq!(std::mem::size_of::<KeyEventRecord>(), 16);
        assert_eq!(std::mem::size_of::<MouseEventRecord>(), 16);
        assert_eq!(std::mem::size_of::<InputRecord>(), 20);
        assert_eq!(std::mem::size_of::<ConsoleScreenBufferInfo>(), 22);
    }

    #[test]
    fn alt_gr_and_surrogate_pairs_produce_text() {
        let mut high_surrogate = None;
        let alt_gr = KeyEventRecord {
            key_down: 1,
            repeat_count: 1,
            virtual_key_code: 0x51,
            virtual_scan_code: 0,
            unicode_char: '@' as u16,
            control_key_state: LEFT_CTRL_PRESSED | RIGHT_ALT_PRESSED,
        };
        assert_eq!(
            key_from_record(alt_gr, &mut high_surrogate),
            Some(Key::Char('@'))
        );

        let surrogate = |unit| KeyEventRecord {
            key_down: 1,
            repeat_count: 1,
            virtual_key_code: 0,
            virtual_scan_code: 0,
            unicode_char: unit,
            control_key_state: 0,
        };
        assert_eq!(
            key_from_record(surrogate(0xd83d), &mut high_surrogate),
            None
        );
        assert_eq!(
            key_from_record(surrogate(0xde00), &mut high_surrogate),
            Some(Key::Char('😀'))
        );
    }
}
