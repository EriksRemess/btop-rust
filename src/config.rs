use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use crate::cli::Cli;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessSort {
    CpuLazy,
    CpuDirect,
    Memory,
    Pid,
    Name,
    Command,
    User,
    Threads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GraphSymbol {
    Braille,
    Block,
    Tty,
}

impl ProcessSort {
    pub const ALL: [Self; 8] = [
        Self::Pid,
        Self::Name,
        Self::Command,
        Self::Threads,
        Self::User,
        Self::Memory,
        Self::CpuDirect,
        Self::CpuLazy,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::CpuLazy => "cpu lazy",
            Self::CpuDirect => "cpu direct",
            Self::Memory => "memory",
            Self::Pid => "pid",
            Self::Name => "name",
            Self::Command => "command",
            Self::User => "user",
            Self::Threads => "threads",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub update_ms: u64,
    pub shown: [bool; 4],
    pub process_sort: ProcessSort,
    pub process_reversed: bool,
    pub process_filter: String,
    pub process_per_core: bool,
    pub process_mem_bytes: bool,
    pub process_tree: bool,
    pub pause_processes: bool,
    pub show_disks: bool,
    pub mem_graphs: bool,
    pub net_auto: bool,
    pub net_sync: bool,
    pub net_iface: Option<String>,
    pub base_10_sizes: bool,
    pub rounded_corners: bool,
    pub truecolor: bool,
    pub theme_background: bool,
    pub tty_mode: bool,
    pub low_color: bool,
    pub vim_keys: bool,
    pub disable_mouse: bool,
    pub terminal_sync: bool,
    pub color_theme: String,
    pub themes_dir: Option<PathBuf>,
    pub preset: Option<u8>,
    pub graph_symbol: GraphSymbol,
    pub show_uptime: bool,
    pub show_cpu_frequency: bool,
    pub check_temperature: bool,
    pub show_core_temperature: bool,
    pub cpu_single_graph: bool,
    pub cpu_invert_lower: bool,
    pub clock_format: String,
    pub warnings: Vec<String>,
    values: HashMap<String, String>,
    source_path: Option<PathBuf>,
    read_only: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            update_ms: 2_000,
            shown: [true; 4],
            process_sort: ProcessSort::CpuLazy,
            process_reversed: false,
            process_filter: String::new(),
            process_per_core: false,
            process_mem_bytes: true,
            process_tree: false,
            pause_processes: false,
            show_disks: true,
            mem_graphs: true,
            net_auto: true,
            net_sync: true,
            net_iface: None,
            base_10_sizes: false,
            rounded_corners: true,
            truecolor: true,
            theme_background: true,
            tty_mode: false,
            low_color: false,
            vim_keys: false,
            disable_mouse: false,
            terminal_sync: true,
            color_theme: "Default".into(),
            themes_dir: None,
            preset: None,
            graph_symbol: GraphSymbol::Braille,
            show_uptime: true,
            show_cpu_frequency: true,
            check_temperature: true,
            show_core_temperature: true,
            cpu_single_graph: false,
            cpu_invert_lower: true,
            clock_format: "%X".into(),
            warnings: Vec::new(),
            values: parse_file(Self::default_file()),
            source_path: default_path(),
            read_only: false,
        }
    }
}

impl Config {
    pub fn load(explicit: Option<&Path>) -> Result<Self, String> {
        let path = explicit.map(Path::to_path_buf).or_else(default_path);
        let Some(path) = path else {
            return Ok(Self::default());
        };
        let text = match fs::read_to_string(&path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let mut config = Self {
                    source_path: Some(path.clone()),
                    ..Self::default()
                };
                config.set_read_only_from(&path);
                return Ok(config);
            }
            Err(error) => return Err(format!("could not read {}: {error}", path.display())),
        };
        let overrides = parse_file(&text);
        let mut config = Self::default();
        for (name, value) in overrides {
            if !config.values.contains_key(&name) {
                continue;
            }
            match valid_config_value(&name, &value) {
                Ok(()) => {
                    config.values.insert(name, value);
                }
                Err(warning) => config.warnings.push(warning),
            }
        }
        config.source_path = Some(path.clone());
        config.set_read_only_from(&path);
        let values = config.values.clone();
        config.update_ms = integer(&values, "update_ms")
            .unwrap_or(config.update_ms)
            .clamp(100, 86_400_000);
        if let Some(value) = values.get("shown_boxes") {
            config.shown = ["cpu", "mem", "net", "proc"]
                .map(|name| value.split_whitespace().any(|item| item == name));
        }
        config.process_sort = match string(&values, "proc_sorting").as_deref() {
            Some("memory") => ProcessSort::Memory,
            Some("pid") => ProcessSort::Pid,
            Some("name") | Some("program") => ProcessSort::Name,
            Some("command") | Some("arguments") => ProcessSort::Command,
            Some("user") => ProcessSort::User,
            Some("threads") => ProcessSort::Threads,
            Some("cpu direct") => ProcessSort::CpuDirect,
            _ => ProcessSort::CpuLazy,
        };
        config.process_reversed =
            boolean(&values, "proc_reversed").unwrap_or(config.process_reversed);
        config.process_filter = string(&values, "proc_filter").unwrap_or_default();
        config.process_per_core =
            boolean(&values, "proc_per_core").unwrap_or(config.process_per_core);
        config.process_mem_bytes =
            boolean(&values, "proc_mem_bytes").unwrap_or(config.process_mem_bytes);
        config.process_tree = boolean(&values, "proc_tree").unwrap_or(config.process_tree);
        config.show_disks = boolean(&values, "show_disks").unwrap_or(config.show_disks);
        config.mem_graphs = boolean(&values, "mem_graphs").unwrap_or(config.mem_graphs);
        config.net_auto = boolean(&values, "net_auto").unwrap_or(config.net_auto);
        config.net_sync = boolean(&values, "net_sync").unwrap_or(config.net_sync);
        config.net_iface = string(&values, "net_iface").filter(|s| !s.is_empty());
        config.base_10_sizes = boolean(&values, "base_10_sizes").unwrap_or(config.base_10_sizes);
        config.rounded_corners =
            boolean(&values, "rounded_corners").unwrap_or(config.rounded_corners);
        config.truecolor = boolean(&values, "truecolor").unwrap_or(config.truecolor);
        config.theme_background =
            boolean(&values, "theme_background").unwrap_or(config.theme_background);
        config.tty_mode = boolean(&values, "force_tty").unwrap_or(config.tty_mode);
        config.vim_keys = boolean(&values, "vim_keys").unwrap_or(config.vim_keys);
        config.disable_mouse = boolean(&values, "disable_mouse").unwrap_or(config.disable_mouse);
        config.terminal_sync = boolean(&values, "terminal_sync").unwrap_or(config.terminal_sync);
        config.color_theme = string(&values, "color_theme").unwrap_or(config.color_theme);
        config.graph_symbol = match string(&values, "graph_symbol").as_deref() {
            Some("block") => GraphSymbol::Block,
            Some("tty") => GraphSymbol::Tty,
            _ => GraphSymbol::Braille,
        };
        config.show_uptime = boolean(&values, "show_uptime").unwrap_or(config.show_uptime);
        config.show_cpu_frequency =
            boolean(&values, "show_cpu_freq").unwrap_or(config.show_cpu_frequency);
        config.check_temperature =
            boolean(&values, "check_temp").unwrap_or(config.check_temperature);
        config.show_core_temperature =
            boolean(&values, "show_coretemp").unwrap_or(config.show_core_temperature);
        config.cpu_single_graph =
            boolean(&values, "cpu_single_graph").unwrap_or(config.cpu_single_graph);
        config.cpu_invert_lower =
            boolean(&values, "cpu_invert_lower").unwrap_or(config.cpu_invert_lower);
        config.clock_format = string(&values, "clock_format").unwrap_or(config.clock_format);
        Ok(config)
    }

    pub fn apply_cli(&mut self, cli: &Cli) {
        if let Some(update_ms) = cli.update_ms {
            self.update_ms = update_ms;
        }
        if let Some(filter) = &cli.filter {
            self.process_filter.clone_from(filter);
        }
        if let Some(force_tty) = cli.force_tty {
            self.tty_mode = force_tty;
        }
        self.low_color |= cli.low_color;
        self.themes_dir.clone_from(&cli.themes_dir);
        if let Some(preset) = cli.preset {
            self.apply_preset(preset);
        }
    }

    pub fn apply_preset(&mut self, preset: u8) {
        if !self.enabled_presets().contains(&preset) {
            return;
        }
        let Some(definition) = self.preset_definitions().get(preset as usize).cloned() else {
            return;
        };
        let mut boxes = Vec::new();
        for entry in definition.split(',') {
            let mut values = entry.split(':');
            let Some(name) = values.next() else { continue };
            let Some(position) = values.next() else {
                continue;
            };
            let Some(graph) = values.next() else { continue };
            boxes.push(name);
            match name {
                "cpu" => self.set_value("cpu_bottom", (position != "0").to_string()),
                "mem" => self.set_value("mem_below_net", (position != "0").to_string()),
                "proc" => self.set_value("proc_left", (position != "0").to_string()),
                _ => {}
            }
            let graph_option = if name.starts_with("gpu") {
                "graph_symbol_gpu".to_string()
            } else {
                format!("graph_symbol_{name}")
            };
            self.set_value(&graph_option, graph);
        }
        self.set_value("shown_boxes", boxes.join(" "));
        self.preset = Some(preset);
    }

    pub fn cycle_preset(&mut self, forward: bool) {
        let presets = self.enabled_presets();
        if presets.is_empty() {
            return;
        }
        let current = self
            .preset
            .and_then(|preset| presets.iter().position(|candidate| *candidate == preset));
        let next = match (current, forward) {
            (Some(index), true) => (index + 1) % presets.len(),
            (Some(index), false) => (index + presets.len() - 1) % presets.len(),
            (None, true) => 0,
            (None, false) => presets.len() - 1,
        };
        self.apply_preset(presets[next]);
    }

    fn preset_definitions(&self) -> Vec<String> {
        let mut presets = vec!["cpu:0:default,mem:0:default,net:0:default,proc:0:default".into()];
        presets.extend(
            self.value("presets")
                .unwrap_or_default()
                .split_whitespace()
                .take(9)
                .filter(|preset| valid_preset(preset))
                .map(str::to_string),
        );
        presets
    }

    fn enabled_presets(&self) -> Vec<u8> {
        let disabled = self.value("disable_presets").unwrap_or("Off");
        let count = self.preset_definitions().len();
        (0..count)
            .filter(|index| {
                !matches!(disabled, "All")
                    && !(matches!(disabled, "Default") && *index == 0)
                    && !(matches!(disabled, "Custom") && *index > 0)
            })
            .map(|index| index as u8)
            .collect()
    }

    pub fn default_file() -> &'static str {
        include_str!("../assets/default_btop.conf")
    }

    pub fn value(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }

    pub fn bool_value(&self, key: &str) -> Option<bool> {
        self.value(key).and_then(parse_bool)
    }

    pub fn set_value(&mut self, key: &str, value: impl Into<String>) {
        let value = value.into();
        self.values.insert(key.to_string(), value.clone());
        match key {
            "update_ms" => {
                if let Ok(number) = value.parse::<u64>() {
                    self.update_ms = number.clamp(100, 86_400_000);
                }
            }
            "proc_sorting" => {
                self.process_sort = match value.as_str() {
                    "memory" => ProcessSort::Memory,
                    "pid" => ProcessSort::Pid,
                    "name" | "program" => ProcessSort::Name,
                    "command" | "arguments" => ProcessSort::Command,
                    "user" => ProcessSort::User,
                    "threads" => ProcessSort::Threads,
                    "cpu direct" => ProcessSort::CpuDirect,
                    _ => ProcessSort::CpuLazy,
                };
            }
            "proc_reversed" => set_bool(&value, &mut self.process_reversed),
            "proc_per_core" => set_bool(&value, &mut self.process_per_core),
            "proc_mem_bytes" => set_bool(&value, &mut self.process_mem_bytes),
            "proc_tree" => set_bool(&value, &mut self.process_tree),
            "show_disks" => set_bool(&value, &mut self.show_disks),
            "mem_graphs" => set_bool(&value, &mut self.mem_graphs),
            "net_auto" => set_bool(&value, &mut self.net_auto),
            "net_sync" => set_bool(&value, &mut self.net_sync),
            "net_iface" => self.net_iface = (!value.is_empty()).then_some(value),
            "base_10_sizes" => set_bool(&value, &mut self.base_10_sizes),
            "rounded_corners" => set_bool(&value, &mut self.rounded_corners),
            "truecolor" => set_bool(&value, &mut self.truecolor),
            "theme_background" => set_bool(&value, &mut self.theme_background),
            "force_tty" => set_bool(&value, &mut self.tty_mode),
            "vim_keys" => set_bool(&value, &mut self.vim_keys),
            "disable_mouse" => set_bool(&value, &mut self.disable_mouse),
            "terminal_sync" => set_bool(&value, &mut self.terminal_sync),
            "color_theme" => self.color_theme = value,
            "graph_symbol" => {
                self.graph_symbol = match value.as_str() {
                    "block" => GraphSymbol::Block,
                    "tty" => GraphSymbol::Tty,
                    _ => GraphSymbol::Braille,
                };
            }
            "show_uptime" => set_bool(&value, &mut self.show_uptime),
            "show_cpu_freq" => set_bool(&value, &mut self.show_cpu_frequency),
            "check_temp" => set_bool(&value, &mut self.check_temperature),
            "show_coretemp" => set_bool(&value, &mut self.show_core_temperature),
            "cpu_single_graph" => set_bool(&value, &mut self.cpu_single_graph),
            "cpu_invert_lower" => set_bool(&value, &mut self.cpu_invert_lower),
            "clock_format" => self.clock_format = value,
            "shown_boxes" => {
                self.shown = ["cpu", "mem", "net", "proc"]
                    .map(|name| value.split_whitespace().any(|item| item == name));
            }
            _ => {}
        }
    }

    pub fn flip_value(&mut self, key: &str) {
        if let Some(value) = self.bool_value(key) {
            self.set_value(key, (!value).to_string());
        }
    }

    pub fn toggle_box(&mut self, name: &str) {
        let mut boxes: Vec<String> = self
            .value("shown_boxes")
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        if let Some(position) = boxes.iter().position(|box_name| box_name == name) {
            boxes.remove(position);
        } else {
            boxes.push(name.to_string());
        }
        self.values.insert("shown_boxes".into(), boxes.join(" "));
        for (index, box_name) in ["cpu", "mem", "net", "proc"].iter().enumerate() {
            self.shown[index] = boxes.iter().any(|value| value == box_name);
        }
    }

    pub fn reload(&mut self) -> Result<(), String> {
        let path = self.source_path.clone();
        *self = Self::load(path.as_deref())?;
        Ok(())
    }

    pub fn save(&mut self) -> Result<(), String> {
        if !self
            .value("save_config_on_exit")
            .and_then(parse_bool)
            .unwrap_or(true)
        {
            return Ok(());
        }
        if self.read_only {
            return Ok(());
        }
        self.sync_values();
        let Some(path) = self.source_path.as_ref() else {
            return Ok(());
        };
        let template = Self::default_file();
        let mut output = String::with_capacity(template.len());
        for line in template.lines() {
            let key = line
                .split_once('=')
                .map(|(key, _)| key.trim())
                .filter(|key| !key.is_empty() && !key.starts_with('#'));
            if let Some((key, value)) = key.and_then(|key| self.values.get(key).map(|v| (key, v))) {
                output.push_str(key);
                output.push_str(" = ");
                let quote = line
                    .split_once('=')
                    .is_some_and(|(_, default)| default.trim_start().starts_with('"'));
                if quote {
                    output.push('"');
                }
                output.push_str(value);
                if quote {
                    output.push('"');
                }
            } else {
                output.push_str(line);
            }
            output.push('\n');
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| format!("could not create {}: {error}", parent.display()))?;
        }
        let temporary = path.with_extension("conf.tmp");
        fs::write(&temporary, output)
            .map_err(|error| format!("could not write {}: {error}", temporary.display()))?;
        fs::rename(&temporary, path)
            .map_err(|error| format!("could not replace {}: {error}", path.display()))
    }

    fn set_read_only_from(&mut self, path: &Path) {
        let Some(parent) = path.parent() else {
            return;
        };
        let Ok(metadata) = fs::metadata(parent) else {
            return;
        };
        if metadata.is_dir() && metadata.permissions().mode() & 0o200 == 0 {
            self.read_only = true;
            self.warnings.push(format!(
                "`{}` is not writable; config changes are not persistent",
                parent.display()
            ));
        }
    }

    fn sync_values(&mut self) {
        let mut set = |key: &str, value: String| {
            self.values.insert(key.to_string(), value);
        };
        set("update_ms", self.update_ms.to_string());
        set("proc_sorting", self.process_sort.label().into());
        set("proc_reversed", self.process_reversed.to_string());
        set("proc_filter", self.process_filter.clone());
        set("proc_per_core", self.process_per_core.to_string());
        set("proc_mem_bytes", self.process_mem_bytes.to_string());
        set("proc_tree", self.process_tree.to_string());
        set("show_disks", self.show_disks.to_string());
        set("mem_graphs", self.mem_graphs.to_string());
        set("net_auto", self.net_auto.to_string());
        set("net_sync", self.net_sync.to_string());
        set("net_iface", self.net_iface.clone().unwrap_or_default());
        set("base_10_sizes", self.base_10_sizes.to_string());
        set("rounded_corners", self.rounded_corners.to_string());
        set("truecolor", self.truecolor.to_string());
        set("theme_background", self.theme_background.to_string());
        set("vim_keys", self.vim_keys.to_string());
        set("disable_mouse", self.disable_mouse.to_string());
        set("terminal_sync", self.terminal_sync.to_string());
        set("color_theme", self.color_theme.clone());
        set("show_uptime", self.show_uptime.to_string());
        set("show_cpu_freq", self.show_cpu_frequency.to_string());
        set("check_temp", self.check_temperature.to_string());
        set("show_coretemp", self.show_core_temperature.to_string());
        set("cpu_single_graph", self.cpu_single_graph.to_string());
        set("cpu_invert_lower", self.cpu_invert_lower.to_string());
        set("clock_format", self.clock_format.clone());
    }
}

fn default_path() -> Option<PathBuf> {
    if let Some(path) = std::env::var_os("XDG_CONFIG_HOME") {
        Some(PathBuf::from(path).join("btop/btop.conf"))
    } else {
        std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config/btop/btop.conf"))
    }
}

fn parse_file(text: &str) -> HashMap<String, String> {
    text.lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let (key, value) = line.split_once('=')?;
            Some((
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

fn string(values: &HashMap<String, String>, key: &str) -> Option<String> {
    values.get(key).cloned()
}
fn integer(values: &HashMap<String, String>, key: &str) -> Option<u64> {
    values.get(key)?.parse().ok()
}
fn boolean(values: &HashMap<String, String>, key: &str) -> Option<bool> {
    parse_bool(values.get(key)?)
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.to_ascii_lowercase().as_str() {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn valid_config_value(name: &str, value: &str) -> Result<(), String> {
    const BOOLS: &[&str] = &[
        "theme_background",
        "truecolor",
        "force_tty",
        "vim_keys",
        "disable_mouse",
        "rounded_corners",
        "terminal_sync",
        "proc_reversed",
        "proc_tree",
        "proc_colors",
        "proc_gradient",
        "proc_per_core",
        "proc_mem_bytes",
        "proc_cpu_graphs",
        "proc_info_smaps",
        "proc_left",
        "proc_filter_kernel",
        "proc_follow_detailed",
        "proc_aggregate",
        "keep_dead_proc_usage",
        "cpu_invert_lower",
        "cpu_single_graph",
        "cpu_bottom",
        "show_uptime",
        "show_cpu_watts",
        "check_temp",
        "show_coretemp",
        "base_10_sizes",
        "show_cpu_freq",
        "background_update",
        "mem_graphs",
        "mem_below_net",
        "zfs_arc_cached",
        "show_swap",
        "swap_disk",
        "show_disks",
        "only_physical",
        "use_fstab",
        "zfs_hide_datasets",
        "disk_free_priv",
        "show_io_stat",
        "io_mode",
        "io_graph_combined",
        "swap_upload_download",
        "net_auto",
        "net_sync",
        "show_battery",
        "show_battery_watts",
        "save_config_on_exit",
        "nvml_measure_pcie_speeds",
        "rsmi_measure_pcie_speeds",
        "gpu_mirror_graph",
    ];
    if BOOLS.contains(&name) {
        return parse_bool(value)
            .map(|_| ())
            .ok_or_else(|| format!("Got an invalid bool value for config name: {name}"));
    }
    if matches!(
        name,
        "update_ms" | "net_download" | "net_upload" | "proc_tree_auto_collapse"
    ) {
        let number = value
            .parse::<i64>()
            .map_err(|_| "Invalid numerical value!".to_string())?;
        if name == "update_ms" && number < 100 {
            return Err("Config value update_ms set too low (<100).".into());
        }
        if name == "update_ms" && number > 86_400_000 {
            return Err("Config value update_ms set too high (>86400000).".into());
        }
        if name == "proc_tree_auto_collapse" && !(0..=10_000).contains(&number) {
            return Err("Config value proc_tree_auto_collapse must be between 0 and 10000.".into());
        }
        return Ok(());
    }
    let valid_choice = |choices: &[&str]| choices.contains(&value);
    let valid = match name {
        "log_level" => valid_choice(&["DISABLED", "ERROR", "WARNING", "INFO", "DEBUG"]),
        "graph_symbol" => valid_choice(&["braille", "block", "tty"]),
        name if name.starts_with("graph_symbol_") => {
            valid_choice(&["default", "braille", "block", "tty"])
        }
        "show_gpu_info" => valid_choice(&["Auto", "On", "Off"]),
        "disable_presets" => valid_choice(&["Off", "Default", "Custom", "All"]),
        "base_10_bitrate" => valid_choice(&["Auto", "True", "False"]),
        "freq_mode" => valid_choice(&["first", "range", "lowest", "highest", "average"]),
        "temp_scale" => valid_choice(&["celsius", "fahrenheit", "kelvin", "rankine"]),
        "proc_sorting" => valid_choice(&[
            "pid",
            "name",
            "command",
            "threads",
            "user",
            "memory",
            "cpu direct",
            "cpu lazy",
        ]),
        "shown_boxes" => {
            !value.is_empty()
                && value.split_whitespace().all(|item| {
                    matches!(item, "cpu" | "mem" | "net" | "proc")
                        || item
                            .strip_prefix("gpu")
                            .and_then(|index| index.parse::<u8>().ok())
                            .is_some_and(|index| index < 6)
                })
        }
        "presets" => value.split_whitespace().all(valid_preset),
        "cpu_core_map" => value.split_whitespace().all(|mapping| {
            mapping.split_once(':').is_some_and(|(cpu, sensor)| {
                cpu.parse::<u32>().is_ok() && sensor.parse::<u32>().is_ok()
            })
        }),
        "io_graph_speeds" => value.split_whitespace().all(|mapping| {
            mapping
                .split_once(':')
                .is_some_and(|(disk, speed)| !disk.is_empty() && speed.parse::<u64>().is_ok())
        }),
        _ => true,
    };
    valid
        .then_some(())
        .ok_or_else(|| format!("Invalid value for config name {name}: {value}"))
}

fn set_bool(value: &str, target: &mut bool) {
    if let Some(parsed) = parse_bool(value) {
        *target = parsed;
    }
}

fn valid_preset(preset: &str) -> bool {
    let boxes: Vec<_> = preset.split(',').collect();
    !boxes.is_empty()
        && boxes.len() <= 10
        && boxes.iter().all(|entry| {
            let values: Vec<_> = entry.split(':').collect();
            values.len() == 3
                && (matches!(values[0], "cpu" | "mem" | "net" | "proc")
                    || values[0]
                        .strip_prefix("gpu")
                        .and_then(|index| index.parse::<u8>().ok())
                        .is_some_and(|index| index < 6))
                && matches!(values[1], "0" | "1")
                && matches!(values[2], "default" | "braille" | "block" | "tty")
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_contract_contains_all_reference_keys() {
        let values = parse_file(Config::default_file());
        assert_eq!(values.len(), 89);
        assert_eq!(
            values.get("color_theme").map(String::as_str),
            Some("Default")
        );
        assert!(values.contains_key("custom_gpu_name5"));
    }

    #[test]
    fn saves_live_values_using_the_current_reference_schema() {
        let path =
            std::env::temp_dir().join(format!("btop-rust-config-test-{}.conf", std::process::id()));
        fs::write(
            &path,
            "update_ms = 2000\nsave_config_on_exit = true\nfuture_option = \"kept\"\n",
        )
        .unwrap();
        let mut config = Config::load(Some(&path)).unwrap();
        config.update_ms = 3100;
        config.save().unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("update_ms = 3100"));
        assert!(!saved.contains("future_option"));
        assert_eq!(parse_file(&saved).len(), 89);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn missing_explicit_config_is_created_like_btop() {
        let path = std::env::temp_dir().join(format!(
            "btop-rust-new-config-test-{}.conf",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        let mut config = Config::load(Some(&path)).unwrap();
        config.save().unwrap();
        let saved = fs::read_to_string(&path).unwrap();
        assert!(saved.contains("update_ms = 2000"));
        let _ = fs::remove_file(path);
    }

    #[test]
    fn readable_config_in_read_only_directory_is_loaded_but_not_rewritten() {
        let directory = std::env::temp_dir().join(format!(
            "btop-rust-read-only-config-test-{}",
            std::process::id()
        ));
        let path = directory.join("btop.conf");
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir(&directory).unwrap();
        fs::write(&path, "update_ms = 1700\nsave_config_on_exit = true\n").unwrap();
        fs::set_permissions(&directory, fs::Permissions::from_mode(0o555)).unwrap();

        let mut config = Config::load(Some(&path)).unwrap();
        assert_eq!(config.update_ms, 1700);
        assert!(config.read_only);
        assert!(config.warnings.iter().any(|warning| {
            warning.contains("not writable") && warning.contains("not persistent")
        }));
        config.update_ms = 3100;
        config.save().unwrap();
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            "update_ms = 1700\nsave_config_on_exit = true\n"
        );

        fs::set_permissions(&directory, fs::Permissions::from_mode(0o755)).unwrap();
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn box_toggles_preserve_gpu_panel_tokens() {
        let mut config = Config::default();
        config
            .values
            .insert("shown_boxes".into(), "cpu proc gpu0 mem net".into());
        config.toggle_box("mem");
        assert_eq!(config.value("shown_boxes"), Some("cpu proc gpu0 net"));
        assert!(!config.shown[1]);
        config.toggle_box("gpu1");
        assert_eq!(config.value("shown_boxes"), Some("cpu proc gpu0 net gpu1"));
    }

    #[test]
    fn configurable_presets_apply_positions_graphs_and_disable_modes() {
        let mut config = Config::default();
        config.set_value(
            "presets",
            "cpu:1:block,proc:1:tty mem:0:braille,net:0:default",
        );
        config.apply_preset(1);
        assert_eq!(config.value("shown_boxes"), Some("cpu proc"));
        assert_eq!(config.bool_value("cpu_bottom"), Some(true));
        assert_eq!(config.bool_value("proc_left"), Some(true));
        assert_eq!(config.value("graph_symbol_cpu"), Some("block"));
        assert_eq!(config.value("graph_symbol_proc"), Some("tty"));

        config.set_value("disable_presets", "Custom");
        config.apply_preset(2);
        assert_eq!(config.preset, Some(1));
        config.apply_preset(0);
        assert_eq!(config.value("shown_boxes"), Some("cpu mem net proc"));
    }
}
