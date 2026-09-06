#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Main,
    GraphText,
    Title,
    Hi,
    Inactive,
    Div,
    CpuBox,
    MemBox,
    NetBox,
    ProcBox,
    Selected,
    Followed,
    ProcPause,
    ProcFollow,
    ProcPauseFollow,
    ProcMisc,
    Process(u8),
    Proc(u8),
    ProcColor(u8),
    Cpu(u8),
    Temp(u8),
    Free(u8),
    Cached(u8),
    Available(u8),
    Used(u8),
    Download(u8),
    Upload(u8),
    MeterBg,
    Banner(u8),
    BannerGray(u8),
    MenuNormal(u8),
    Red,
}

type Rgb = (u8, u8, u8);
type StyleColors = (Rgb, Option<Rgb>);

pub const RESET: &str = "\x1b[0m";
pub const MAIN: Style = Style::Main;
pub const GRAPH_TEXT: Style = Style::GraphText;
pub const TITLE: Style = Style::Title;
pub const HI: Style = Style::Hi;
pub const CPU: Style = Style::Cpu(100);
pub const NET: Style = Style::Download(100);
pub const BOX: Style = Style::Div;
pub const LOW: Style = Style::Inactive;
pub const RED: Style = Style::Red;
pub const CPU_BOX: Style = Style::CpuBox;
pub const MEM_BOX: Style = Style::MemBox;
pub const NET_BOX: Style = Style::NetBox;
pub const PROC_BOX: Style = Style::ProcBox;
pub const SELECTED: Style = Style::Selected;
pub const FOLLOWED: Style = Style::Followed;
pub const METER_BG: Style = Style::MeterBg;

include!(concat!(env!("OUT_DIR"), "/bundled_themes.rs"));

#[derive(Clone, Default)]
pub struct Palette {
    overrides: HashMap<String, Rgb>,
    empty: HashSet<String>,
    custom: bool,
}

impl Palette {
    pub fn load(name: &str, custom_dir: Option<&Path>) -> Result<Self, String> {
        Self::load_in(name, theme_directories(custom_dir))
    }

    fn load_in(name: &str, directories: impl IntoIterator<Item = PathBuf>) -> Result<Self, String> {
        if name.is_empty() || name.eq_ignore_ascii_case("default") {
            return Ok(Self::default());
        }
        let requested = Path::new(name);
        let theme_path = requested
            .is_file()
            .then(|| requested.to_path_buf())
            .or_else(|| find_theme(requested, directories));
        let text = if let Some(theme_path) = theme_path {
            match fs::read_to_string(&theme_path) {
                Ok(text) => text,
                Err(error) => match bundled_theme(requested) {
                    Some(text) => text.to_string(),
                    None => {
                        return Err(format!(
                            "could not read theme {}: {error}",
                            theme_path.display()
                        ));
                    }
                },
            }
        } else if let Some(text) = bundled_theme(requested) {
            text.to_string()
        } else {
            return Ok(Self::default());
        };
        Ok(Self::parse(&text))
    }

    fn parse(text: &str) -> Self {
        let mut overrides = HashMap::new();
        let mut empty = HashSet::new();
        for line in text.lines().map(str::trim) {
            let Some(rest) = line.strip_prefix("theme[") else {
                continue;
            };
            let Some((key, value)) = rest.split_once("]=") else {
                continue;
            };
            let value = value.trim().trim_matches('"');
            if value.is_empty() {
                empty.insert(key.to_string());
            } else if let Some(rgb) = parse_hex(value) {
                overrides.insert(key.to_string(), rgb);
            }
        }
        Self {
            overrides,
            empty,
            custom: true,
        }
    }

    fn color(&self, name: &str, fallback: Rgb) -> Rgb {
        self.overrides.get(name).copied().unwrap_or(fallback)
    }

    fn gradient(&self, name: &str, value: u8, start: Rgb, middle: Rgb, end: Rgb) -> Rgb {
        let start = self.color(&format!("{name}_start"), start);
        let end = self.color(&format!("{name}_end"), end);
        if self.empty.contains(&format!("{name}_mid")) {
            interpolate_steps(start, end, value, 100)
        } else {
            gradient(
                value,
                start,
                self.color(&format!("{name}_mid"), middle),
                end,
            )
        }
    }
}

pub fn available_themes(custom_dir: Option<&Path>) -> Vec<String> {
    available_themes_in(theme_directories(custom_dir))
}

fn available_themes_in(directories: impl IntoIterator<Item = PathBuf>) -> Vec<String> {
    let mut themes = vec!["Default".to_string(), "TTY".to_string()];
    let mut names = std::collections::HashSet::new();
    for directory in directories {
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "theme")
                && let Some(name) = path.file_name().and_then(|name| name.to_str())
                && names.insert(name.to_string())
            {
                themes.push(name.to_string());
            }
        }
    }
    for (name, _) in BUNDLED_THEMES {
        if names.insert(name.to_string()) {
            themes.push(name.to_string());
        }
    }
    themes[2..].sort();
    themes
}

fn find_theme(requested: &Path, directories: impl IntoIterator<Item = PathBuf>) -> Option<PathBuf> {
    let requested_file = requested.file_name();
    let requested_stem = requested.file_stem();
    for directory in directories {
        let Ok(files) = fs::read_dir(directory) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path
                .extension()
                .is_some_and(|extension| extension == "theme")
                && (path == requested
                    || path.file_name() == requested_file
                    || path.file_stem() == requested_stem)
            {
                return Some(path);
            }
        }
    }
    None
}

fn bundled_theme(requested: &Path) -> Option<&'static str> {
    BUNDLED_THEMES.iter().find_map(|(name, contents)| {
        let path = Path::new(name);
        (path.file_name() == requested.file_name() || path.file_stem() == requested.file_stem())
            .then_some(*contents)
    })
}

fn user_theme_directory() -> Option<PathBuf> {
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|path| !path.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .map(|directory| directory.join("btoprs/themes"))
}

pub fn install_missing_themes(custom_dir: Option<&Path>) -> std::io::Result<()> {
    if let Some(directory) = user_theme_directory() {
        install_missing_themes_in(&directory, &theme_directories(custom_dir))?;
    }
    Ok(())
}

fn install_missing_themes_in(directory: &Path, search_dirs: &[PathBuf]) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_STAGING: AtomicU64 = AtomicU64::new(0);

    let mut missing = Vec::new();
    for &(name, contents) in BUNDLED_THEMES {
        match fs::symlink_metadata(directory.join(name)) {
            Ok(_) => {} // Preserve user files, including symlinks.
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                // Creating a higher-priority user copy would shadow an existing
                // theme from a custom, legacy, data, or system directory.
                if !search_dirs
                    .iter()
                    .any(|path| fs::symlink_metadata(path.join(name)).is_ok())
                {
                    missing.push((name, contents));
                }
            }
            Err(error) => return Err(error),
        }
    }
    if missing.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(directory)?;
    // Publish complete files without replacing an existing destination, even
    // when two instances start together. Keep staging on the same filesystem.
    struct Staging(PathBuf);
    impl Drop for Staging {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let staging = loop {
        let sequence = NEXT_STAGING.fetch_add(1, Ordering::Relaxed);
        let path = directory.join(format!(".btoprs-themes-{}-{sequence}", std::process::id()));
        match fs::DirBuilder::new().mode(0o700).create(&path) {
            Ok(()) => break Staging(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    };
    for (name, contents) in missing {
        let source = staging.0.join(name);
        fs::write(&source, contents)?;
        match fs::hard_link(&source, directory.join(name)) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn theme_directories(custom_dir: Option<&Path>) -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Some(directory) = custom_dir {
        directories.push(directory.to_path_buf());
    }
    if let Some(directory) = user_theme_directory() {
        directories.push(directory);
    }
    if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME").filter(|path| !path.is_empty()) {
        directories.push(Path::new(&config_home).join("btop/themes"));
    } else if let Some(home) = std::env::var_os("HOME") {
        directories.push(Path::new(&home).join(".config/btop/themes"));
    }
    if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
        directories.push(Path::new(&data_home).join("btoprs/themes"));
        directories.push(Path::new(&data_home).join("btop/themes"));
    } else if let Some(home) = std::env::var_os("HOME") {
        directories.push(Path::new(&home).join(".local/share/btoprs/themes"));
        directories.push(Path::new(&home).join(".local/share/btop/themes"));
    }
    if let Ok(executable) = std::env::current_exe()
        && let Some(prefix) = executable.parent().and_then(Path::parent)
    {
        directories.push(prefix.join("share/btoprs/themes"));
        directories.push(prefix.join("share/btop/themes"));
    }
    directories.push("/usr/local/share/btoprs/themes".into());
    directories.push("/usr/local/share/btop/themes".into());
    directories.push("/usr/share/btoprs/themes".into());
    directories.push("/usr/share/btop/themes".into());
    directories
}

pub fn usage(percent: f64) -> Style {
    Style::Cpu(percent.clamp(0.0, 100.0).round() as u8)
}

pub fn with_value(style: Style, value: usize) -> Style {
    let value = value.min(100) as u8;
    match style {
        Style::Cpu(_) => Style::Cpu(value),
        Style::Temp(_) => Style::Temp(value),
        Style::Free(_) => Style::Free(value),
        Style::Cached(_) => Style::Cached(value),
        Style::Available(_) => Style::Available(value),
        Style::Used(_) => Style::Used(value),
        Style::Download(_) => Style::Download(value),
        Style::Upload(_) => Style::Upload(value),
        Style::Process(_) => Style::Process(value),
        Style::Proc(_) => Style::Proc(value),
        Style::ProcColor(_) => Style::ProcColor(value),
        Style::Banner(_) | Style::BannerGray(_) | Style::MenuNormal(_) => style,
        other => other,
    }
}

pub fn escape(
    style: Style,
    low_color: bool,
    tty: bool,
    theme_background: bool,
    palette: &Palette,
) -> String {
    if tty {
        return tty_escape(style).to_string();
    }
    let (foreground, background) = colors(style, palette);
    let main_background = palette.color("main_bg", (0, 0, 0));
    if low_color {
        let fg = truecolor_to_256(foreground);
        if let Some(background) = background {
            format!("\x1b[38;5;{fg};48;5;{}m", truecolor_to_256(background))
        } else if theme_background {
            format!("\x1b[38;5;{fg};48;5;{}m", truecolor_to_256(main_background))
        } else {
            format!("\x1b[38;5;{fg};49m")
        }
    } else if let Some((r, g, b)) = background {
        format!(
            "\x1b[38;2;{};{};{};48;2;{r};{g};{b}m",
            foreground.0, foreground.1, foreground.2
        )
    } else if theme_background {
        format!(
            "\x1b[38;2;{};{};{};48;2;{};{};{}m",
            foreground.0,
            foreground.1,
            foreground.2,
            main_background.0,
            main_background.1,
            main_background.2
        )
    } else {
        format!(
            "\x1b[38;2;{};{};{};49m",
            foreground.0, foreground.1, foreground.2
        )
    }
}

fn colors(style: Style, palette: &Palette) -> StyleColors {
    match style {
        Style::Main => (palette.color("main_fg", (0xcc, 0xcc, 0xcc)), None),
        Style::GraphText => (palette.color("graph_text", (0x60, 0x60, 0x60)), None),
        Style::Title => (palette.color("title", (0xee, 0xee, 0xee)), None),
        Style::Hi | Style::Red => (palette.color("hi_fg", (0xb5, 0x40, 0x40)), None),
        Style::Inactive => (palette.color("inactive_fg", (0x40, 0x40, 0x40)), None),
        Style::MeterBg => (
            palette.color("meter_bg", palette.color("inactive_fg", (0x40, 0x40, 0x40))),
            None,
        ),
        Style::Banner(line) => {
            let colors = [
                (0xe6, 0x25, 0x25),
                (0xcd, 0x21, 0x21),
                (0xb3, 0x1d, 0x1d),
                (0x9a, 0x19, 0x19),
                (0x80, 0x14, 0x14),
                (0, 0, 0),
            ];
            (colors[line.min(5) as usize], None)
        }
        Style::BannerGray(line) => {
            let value = 120u8.saturating_sub(line.min(5) * 12);
            ((value, value, value), None)
        }
        Style::MenuNormal(line) => {
            let values = [0xcc, 0xaa, 0x80];
            let value = values[line.min(2) as usize];
            ((value, value, value), None)
        }
        Style::Div => (palette.color("div_line", (0x30, 0x30, 0x30)), None),
        Style::CpuBox => (palette.color("cpu_box", (0x55, 0x6d, 0x59)), None),
        Style::MemBox => (palette.color("mem_box", (0x6c, 0x6c, 0x4b)), None),
        Style::NetBox => (palette.color("net_box", (0x5c, 0x58, 0x8d)), None),
        Style::ProcBox => (palette.color("proc_box", (0x80, 0x52, 0x52)), None),
        Style::Selected => (
            palette.color("selected_fg", (0xee, 0xee, 0xee)),
            Some(palette.color("selected_bg", (0x6a, 0x2f, 0x2f))),
        ),
        Style::Followed => (
            palette.color("followed_fg", (0xee, 0xee, 0xee)),
            Some(palette.color("followed_bg", (0x40, 0x40, 0xb5))),
        ),
        Style::ProcPause => (
            palette.color("proc_banner_fg", (0xee, 0xee, 0xee)),
            Some(palette.color("proc_pause_bg", (0xb5, 0x40, 0x40))),
        ),
        Style::ProcFollow => (
            palette.color("proc_banner_fg", (0xee, 0xee, 0xee)),
            Some(palette.color("proc_follow_bg", (0x40, 0x40, 0xb5))),
        ),
        Style::ProcPauseFollow => (
            palette.color("proc_banner_fg", (0xee, 0xee, 0xee)),
            Some(palette.color("proc_banner_bg", (0x7b, 0x40, 0x7b))),
        ),
        Style::ProcMisc => (palette.color("proc_misc", (0x0d, 0xe7, 0x56)), None),
        Style::Process(value) => {
            let color = if !palette.custom || palette.overrides.contains_key("process_start") {
                palette.gradient(
                    "process",
                    value,
                    (0x80, 0xd0, 0xa3),
                    (0xdc, 0xd1, 0x79),
                    (0xd4, 0x54, 0x54),
                )
            } else {
                palette.gradient(
                    "cpu",
                    value,
                    (0x77, 0xca, 0x9b),
                    (0xcb, 0xc0, 0x6c),
                    (0xdc, 0x4c, 0x4c),
                )
            };
            (color, None)
        }
        Style::Proc(value) => (
            interpolate_steps(
                palette.color("main_fg", (0xcc, 0xcc, 0xcc)),
                palette.color("inactive_fg", (0x40, 0x40, 0x40)),
                value,
                100,
            ),
            None,
        ),
        Style::ProcColor(value) => (
            interpolate_steps(
                palette.color("inactive_fg", (0x40, 0x40, 0x40)),
                if palette.custom && !palette.overrides.contains_key("process_start") {
                    palette.color("cpu_start", (0x77, 0xca, 0x9b))
                } else {
                    palette.color("process_start", (0x80, 0xd0, 0xa3))
                },
                value,
                100,
            ),
            None,
        ),
        Style::Cpu(value) => (
            palette.gradient(
                "cpu",
                value,
                (0x77, 0xca, 0x9b),
                (0xcb, 0xc0, 0x6c),
                (0xdc, 0x4c, 0x4c),
            ),
            None,
        ),
        Style::Temp(value) => (
            palette.gradient(
                "temp",
                value,
                (0x48, 0x97, 0xd4),
                (0x54, 0x74, 0xe8),
                (0xff, 0x40, 0xb6),
            ),
            None,
        ),
        Style::Free(value) => (
            palette.gradient(
                "free",
                value,
                (0x38, 0x4f, 0x21),
                (0xb5, 0xe6, 0x85),
                (0xdc, 0xff, 0x85),
            ),
            None,
        ),
        Style::Cached(value) => (
            palette.gradient(
                "cached",
                value,
                (0x16, 0x33, 0x50),
                (0x74, 0xe6, 0xfc),
                (0x26, 0xc5, 0xff),
            ),
            None,
        ),
        Style::Available(value) => (
            palette.gradient(
                "available",
                value,
                (0x4e, 0x3f, 0x0e),
                (0xff, 0xd7, 0x7a),
                (0xff, 0xb8, 0x14),
            ),
            None,
        ),
        Style::Used(value) => (
            palette.gradient(
                "used",
                value,
                (0x59, 0x2b, 0x26),
                (0xd9, 0x62, 0x6d),
                (0xff, 0x47, 0x69),
            ),
            None,
        ),
        Style::Download(value) => (
            palette.gradient(
                "download",
                value,
                (0x29, 0x1f, 0x75),
                (0x4f, 0x43, 0xa3),
                (0xb0, 0xa9, 0xde),
            ),
            None,
        ),
        Style::Upload(value) => (
            palette.gradient(
                "upload",
                value,
                (0x62, 0x06, 0x65),
                (0x7d, 0x41, 0x80),
                (0xdc, 0xaf, 0xde),
            ),
            None,
        ),
    }
}

fn tty_escape(style: Style) -> &'static str {
    match style {
        Style::Main => "\x1b[37;40m",
        Style::GraphText => "\x1b[90;40m",
        Style::Title => "\x1b[97;40m",
        Style::Selected => "\x1b[97;41m",
        Style::Followed => "\x1b[97;44m",
        Style::ProcPause => "\x1b[97;41m",
        Style::ProcFollow => "\x1b[97;44m",
        Style::ProcPauseFollow => "\x1b[97;45m",
        Style::ProcMisc => "\x1b[92;40m",
        Style::Process(value) => {
            if value > 66 {
                "\x1b[31;40m"
            } else if value > 33 {
                "\x1b[33;40m"
            } else {
                "\x1b[32;40m"
            }
        }
        Style::Proc(value) => {
            if value > 66 {
                "\x1b[90;40m"
            } else {
                "\x1b[37;40m"
            }
        }
        Style::ProcColor(value) => {
            if value > 66 {
                "\x1b[32;40m"
            } else {
                "\x1b[90;40m"
            }
        }
        Style::Hi | Style::Red => "\x1b[91;40m",
        Style::Inactive | Style::Div | Style::MeterBg | Style::MenuNormal(_) => "\x1b[90;40m",
        Style::BannerGray(line) => {
            if line > 2 {
                "\x1b[90;40m"
            } else {
                "\x1b[37;40m"
            }
        }
        Style::Banner(line) => {
            if line > 2 {
                "\x1b[31;40m"
            } else {
                "\x1b[91;40m"
            }
        }
        Style::CpuBox => "\x1b[32;40m",
        Style::MemBox => "\x1b[33;40m",
        Style::NetBox => "\x1b[35;40m",
        Style::ProcBox => "\x1b[31;40m",
        Style::Cpu(value) => {
            if value > 66 {
                "\x1b[91;40m"
            } else if value > 33 {
                "\x1b[93;40m"
            } else {
                "\x1b[92;40m"
            }
        }
        Style::Temp(value) => {
            if value > 66 {
                "\x1b[95;40m"
            } else if value > 33 {
                "\x1b[96;40m"
            } else {
                "\x1b[94;40m"
            }
        }
        Style::Free(value) => {
            if value > 66 {
                "\x1b[92;40m"
            } else {
                "\x1b[32;40m"
            }
        }
        Style::Cached(value) => {
            if value > 66 {
                "\x1b[96;40m"
            } else {
                "\x1b[36;40m"
            }
        }
        Style::Available(value) => {
            if value > 66 {
                "\x1b[93;40m"
            } else {
                "\x1b[33;40m"
            }
        }
        Style::Used(value) => {
            if value > 66 {
                "\x1b[91;40m"
            } else {
                "\x1b[31;40m"
            }
        }
        Style::Download(value) => {
            if value > 66 {
                "\x1b[94;40m"
            } else {
                "\x1b[34;40m"
            }
        }
        Style::Upload(value) => {
            if value > 66 {
                "\x1b[95;40m"
            } else {
                "\x1b[35;40m"
            }
        }
    }
}

fn parse_hex(value: &str) -> Option<Rgb> {
    let value = value.strip_prefix('#')?;
    if !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    match value.len() {
        2 => {
            let channel = u8::from_str_radix(value, 16).ok()?;
            Some((channel, channel, channel))
        }
        6 => Some((
            u8::from_str_radix(&value[0..2], 16).ok()?,
            u8::from_str_radix(&value[2..4], 16).ok()?,
            u8::from_str_radix(&value[4..6], 16).ok()?,
        )),
        3 => Some((
            u8::from_str_radix(&value[0..1].repeat(2), 16).ok()?,
            u8::from_str_radix(&value[1..2].repeat(2), 16).ok()?,
            u8::from_str_radix(&value[2..3].repeat(2), 16).ok()?,
        )),
        _ => None,
    }
}

fn gradient(
    value: u8,
    start: (u8, u8, u8),
    middle: (u8, u8, u8),
    end: (u8, u8, u8),
) -> (u8, u8, u8) {
    if value <= 50 {
        interpolate_steps(start, middle, value, 50)
    } else {
        interpolate_steps(middle, end, value - 50, 50)
    }
}

fn interpolate_steps(from: Rgb, to: Rgb, step: u8, range: u8) -> Rgb {
    let channel = |a: u8, b: u8| {
        let delta = i32::from(b) - i32::from(a);
        (i32::from(a) + i32::from(step) * delta / i32::from(range)) as u8
    };
    (
        channel(from.0, to.0),
        channel(from.1, to.1),
        channel(from.2, to.2),
    )
}

fn truecolor_to_256((red, green, blue): (u8, u8, u8)) -> u8 {
    let grey_red = (red as f64 / 11.0).round() as u8;
    if grey_red == (green as f64 / 11.0).round() as u8
        && grey_red == (blue as f64 / 11.0).round() as u8
    {
        232 + grey_red.min(23)
    } else {
        16 + (red as f64 / 51.0).round() as u8 * 36
            + (green as f64 / 51.0).round() as u8 * 6
            + (blue as f64 / 51.0).round() as u8
    }
}
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installing_themes_does_not_shadow_other_search_directories() {
        let root = std::env::temp_dir().join(format!(
            "btoprs-theme-search-install-{}",
            std::process::id()
        ));
        for location in ["legacy", "data", "system", "custom"] {
            let directory = root.join(location).join("user/themes");
            let existing = root.join(location).join("existing/themes");
            fs::create_dir_all(&existing).unwrap();
            let text = "theme[hi_fg]=\"#112233\"\n";
            fs::write(existing.join("dracula.theme"), text).unwrap();
            let search_dirs = if location == "custom" {
                vec![existing.clone(), directory.clone()]
            } else {
                vec![directory.clone(), existing.clone()]
            };
            let before = Palette::load_in("dracula", search_dirs.clone()).unwrap();
            install_missing_themes_in(&directory, &search_dirs).unwrap();
            install_missing_themes_in(&directory, &search_dirs).unwrap();
            assert!(!directory.join("dracula.theme").exists());
            assert_eq!(
                fs::read_to_string(existing.join("dracula.theme")).unwrap(),
                text
            );
            let after = Palette::load_in("dracula", search_dirs).unwrap();
            assert_eq!(before.color("hi_fg", (0, 0, 0)), (0x11, 0x22, 0x33));
            assert_eq!(after.color("hi_fg", (0, 0, 0)), (0x11, 0x22, 0x33));
            assert!(directory.join("ayu.theme").is_file());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn unreadable_bundled_theme_files_use_embedded_palette() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("btoprs-theme-read-fallback-{}", std::process::id()));
        for failure in ["dangling-symlink", "invalid-utf8", "directory"] {
            let directory = root.join(failure);
            fs::create_dir_all(&directory).unwrap();
            let path = directory.join("dracula.theme");
            match failure {
                "dangling-symlink" => symlink(root.join("missing"), &path).unwrap(),
                "invalid-utf8" => fs::write(&path, [0xff]).unwrap(),
                _ => fs::create_dir(&path).unwrap(),
            }
            install_missing_themes_in(&directory, &[]).unwrap();
            for name in ["dracula", "dracula.theme"] {
                let palette = Palette::load_in(name, [directory.clone()]).unwrap();
                assert_eq!(
                    palette.color("hi_fg", (0, 0, 0)),
                    (0x62, 0x72, 0xa4),
                    "{failure}"
                );
            }
            // No embedded copy exists for an unrelated custom theme: retain
            // the read error instead of silently pretending it loaded.
            fs::rename(&path, directory.join("my-custom.theme")).unwrap();
            assert!(Palette::load_in("my-custom", [directory]).is_err());
        }
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn embedded_themes_work_without_any_installed_files() {
        let themes = available_themes_in([]);
        assert_eq!(themes.len(), BUNDLED_THEMES.len() + 2);
        assert_eq!(&themes[..2], ["Default", "TTY"]);
        for name in ["dracula", "dracula.theme"] {
            let palette = Palette::load_in(name, []).unwrap();
            assert_eq!(palette.color("hi_fg", (0, 0, 0)), (0x62, 0x72, 0xa4));
        }
        assert!(
            !Palette::load_in("missing-unknown-theme", [])
                .unwrap()
                .custom
        );
    }

    #[test]
    fn installs_missing_themes_preserving_edits_and_symlinks() {
        use std::os::unix::fs::symlink;
        let root =
            std::env::temp_dir().join(format!("btoprs-theme-install-{}", std::process::id()));
        let directory = root.join("themes");
        fs::create_dir_all(&directory).unwrap();
        let edited = directory.join("dracula.theme");
        let custom = "theme[hi_fg]=\"#112233\"\n";
        fs::write(&edited, custom).unwrap();
        let missing_target = root.join("not-created");
        symlink(&missing_target, directory.join("nord.theme")).unwrap();
        install_missing_themes_in(&directory, &[]).unwrap();
        assert_eq!(fs::read_to_string(&edited).unwrap(), custom);
        assert_eq!(
            fs::read_link(directory.join("nord.theme")).unwrap(),
            missing_target
        );
        assert!(!missing_target.exists());
        let palette = Palette::load_in("dracula", [directory.clone()]).unwrap();
        assert_eq!(palette.color("hi_fg", (0, 0, 0)), (0x11, 0x22, 0x33));
        for &(name, contents) in BUNDLED_THEMES {
            if name != "dracula.theme" && name != "nord.theme" {
                assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), contents);
            }
        }
        fs::remove_file(directory.join("ayu.theme")).unwrap();
        install_missing_themes_in(&directory, &[]).unwrap();
        assert_eq!(
            fs::read_to_string(directory.join("ayu.theme")).unwrap(),
            bundled_theme(Path::new("ayu")).unwrap()
        );
        assert_eq!(fs::read_to_string(edited).unwrap(), custom);
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            BUNDLED_THEMES.len()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn concurrent_theme_installation_publishes_complete_files() {
        let directory =
            std::env::temp_dir().join(format!("btoprs-theme-concurrent-{}", std::process::id()));
        std::thread::scope(|scope| {
            for _ in 0..4 {
                let directory = &directory;
                scope.spawn(move || install_missing_themes_in(directory, &[]).unwrap());
            }
        });
        for &(name, contents) in BUNDLED_THEMES {
            assert_eq!(fs::read_to_string(directory.join(name)).unwrap(), contents);
        }
        assert_eq!(
            fs::read_dir(&directory).unwrap().count(),
            BUNDLED_THEMES.len()
        );
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn installation_failure_does_not_disable_embedded_themes() {
        let root =
            std::env::temp_dir().join(format!("btoprs-theme-unwritable-{}", std::process::id()));
        fs::write(&root, "file instead of directory").unwrap();
        assert!(install_missing_themes_in(&root.join("themes"), &[]).is_err());
        let palette = Palette::load_in("dracula", [root.join("themes")]).unwrap();
        assert_eq!(palette.color("hi_fg", (0, 0, 0)), (0x62, 0x72, 0xa4));
        fs::remove_file(root).unwrap();
    }

    #[test]
    fn malformed_unicode_colors_are_ignored() {
        for value in ["#€aaa", "#€", "#éa", "#aé", "#abéef", "#zzzzzz"] {
            assert_eq!(parse_hex(value), None, "{value}");
        }
        assert_eq!(parse_hex("#aBc"), Some((0xaa, 0xbb, 0xcc)));
        assert_eq!(parse_hex("#12AbEF"), Some((0x12, 0xab, 0xef)));
    }

    #[test]
    fn bundled_themes_are_discovered_and_loaded() {
        let themes = available_themes(None);
        assert!(themes.iter().any(|name| name == "dracula.theme"));
        for name in [
            "catppuccin-frappe.theme",
            "catppuccin-latte.theme",
            "catppuccin-macchiato.theme",
            "catppuccin-mocha.theme",
        ] {
            assert!(themes.iter().any(|candidate| candidate == name));
        }

        let palette = Palette::load("dracula.theme", None).expect("load bundled theme");
        assert_eq!(palette.color("hi_fg", (0, 0, 0)), (0x62, 0x72, 0xa4));
    }

    #[test]
    fn duplicate_legacy_theme_is_listed_once_and_btoprs_copy_wins() {
        let root =
            std::env::temp_dir().join(format!("btoprs-theme-dedup-test-{}", std::process::id()));
        let primary = root.join("btoprs/themes");
        let legacy = root.join("btop/themes");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&primary).unwrap();
        fs::create_dir_all(&legacy).unwrap();
        fs::write(primary.join("shared.theme"), "theme[hi_fg]=\"#112233\"\n").unwrap();
        fs::write(legacy.join("shared.theme"), "theme[hi_fg]=\"#aabbcc\"\n").unwrap();

        let directories = vec![primary.clone(), legacy];
        let themes = available_themes_in(directories.clone());
        assert_eq!(
            themes.iter().filter(|name| *name == "shared.theme").count(),
            1
        );
        assert_eq!(
            find_theme(Path::new("shared.theme"), directories),
            Some(primary.join("shared.theme"))
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn every_bundled_theme_parses_its_core_palette() {
        assert_eq!(BUNDLED_THEMES.len(), 45);
        for &(name, contents) in BUNDLED_THEMES {
            let palette = Palette::parse(contents);
            for key in [
                "main_fg", "title", "hi_fg", "cpu_box", "mem_box", "net_box", "proc_box",
            ] {
                assert!(
                    palette.overrides.contains_key(key),
                    "{} is missing {key}",
                    name
                );
            }
        }
    }

    #[test]
    fn tty_theme_uses_the_ansi_palette() {
        assert_eq!(
            escape(Style::CpuBox, false, true, true, &Palette::default()),
            "\x1b[32;40m"
        );
        assert_eq!(
            escape(Style::Temp(0), false, true, true, &Palette::default()),
            "\x1b[94;40m"
        );
        assert_eq!(
            escape(Style::Temp(50), false, true, true, &Palette::default()),
            "\x1b[96;40m"
        );
        assert_eq!(
            escape(Style::Temp(100), false, true, true, &Palette::default()),
            "\x1b[95;40m"
        );
        assert_eq!(
            escape(Style::ProcPause, false, true, true, &Palette::default()),
            "\x1b[97;41m"
        );
    }

    #[test]
    fn empty_theme_midpoint_interpolates_directly_like_btop() {
        let palette = Palette {
            overrides: HashMap::from([
                ("cpu_start".into(), (10, 20, 30)),
                ("cpu_end".into(), (110, 220, 130)),
            ]),
            empty: HashSet::from(["cpu_mid".into()]),
            custom: true,
        };

        assert_eq!(colors(Style::Cpu(50), &palette).0, (60, 120, 80));
        assert_eq!(colors(Style::Cpu(1), &palette).0, (11, 22, 31));
    }

    #[test]
    fn builtin_colors_match_the_btop_default_theme() {
        let palette = Palette::default();
        assert_eq!(colors(Style::GraphText, &palette).0, (0x60, 0x60, 0x60));
        assert_eq!(colors(Style::Temp(0), &palette).0, (0x48, 0x97, 0xd4));
        assert_eq!(colors(Style::Temp(50), &palette).0, (0x54, 0x74, 0xe8));
        assert_eq!(colors(Style::Temp(100), &palette).0, (0xff, 0x40, 0xb6));
        assert_eq!(colors(Style::Process(0), &palette).0, (0x80, 0xd0, 0xa3));
        assert_eq!(colors(Style::Proc(50), &palette).0, (0x86, 0x86, 0x86));
        assert_eq!(
            colors(Style::ProcColor(100), &palette).0,
            (0x80, 0xd0, 0xa3)
        );
        assert_eq!(
            colors(Style::ProcPause, &palette).1,
            Some((0xb5, 0x40, 0x40))
        );
        assert_eq!(
            colors(Style::ProcFollow, &palette).1,
            Some((0x40, 0x40, 0xb5))
        );
        assert_eq!(
            colors(Style::ProcPauseFollow, &palette).1,
            Some((0x7b, 0x40, 0x7b))
        );
    }
}
