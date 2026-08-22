use std::cmp::Ordering;
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Instant;

use crate::collect::{NetworkSample, ProcessSample, Sample, temperature_sensor_names};
use crate::config::{Config, GraphSymbol, ProcessSort};
use crate::gpu::GpuSample;
use crate::terminal::{Key, Size};
use crate::{theme, units};

const HISTORY: usize = 240;

const OPTIONS_GENERAL: &[&str] = &[
    "color_theme",
    "theme_background",
    "truecolor",
    "force_tty",
    "vim_keys",
    "disable_mouse",
    "disable_presets",
    "presets",
    "shown_boxes",
    "update_ms",
    "rounded_corners",
    "terminal_sync",
    "graph_symbol",
    "clock_format",
    "base_10_sizes",
    "background_update",
    "show_battery",
    "selected_battery",
    "show_battery_watts",
    "log_level",
    "save_config_on_exit",
];
const OPTIONS_CPU: &[&str] = &[
    "cpu_bottom",
    "graph_symbol_cpu",
    "cpu_graph_upper",
    "cpu_graph_lower",
    "cpu_invert_lower",
    "cpu_single_graph",
    "show_gpu_info",
    "check_temp",
    "cpu_sensor",
    "show_coretemp",
    "cpu_core_map",
    "temp_scale",
    "show_cpu_freq",
    "freq_mode",
    "custom_cpu_name",
    "show_uptime",
    "show_cpu_watts",
];
const OPTIONS_GPU: &[&str] = &[
    "nvml_measure_pcie_speeds",
    "rsmi_measure_pcie_speeds",
    "graph_symbol_gpu",
    "gpu_mirror_graph",
    "shown_gpus",
    "custom_gpu_name0",
    "custom_gpu_name1",
    "custom_gpu_name2",
    "custom_gpu_name3",
    "custom_gpu_name4",
    "custom_gpu_name5",
];
const OPTIONS_MEM: &[&str] = &[
    "mem_below_net",
    "graph_symbol_mem",
    "mem_graphs",
    "show_disks",
    "show_io_stat",
    "io_mode",
    "io_graph_combined",
    "io_graph_speeds",
    "show_swap",
    "swap_disk",
    "only_physical",
    "use_fstab",
    "zfs_hide_datasets",
    "disk_free_priv",
    "disks_filter",
    "zfs_arc_cached",
];
const OPTIONS_NET: &[&str] = &[
    "graph_symbol_net",
    "swap_upload_download",
    "net_download",
    "net_upload",
    "net_auto",
    "net_sync",
    "net_iface",
    "base_10_bitrate",
];
const OPTIONS_PROC: &[&str] = &[
    "proc_left",
    "graph_symbol_proc",
    "proc_sorting",
    "proc_reversed",
    "proc_tree",
    "proc_aggregate",
    "proc_tree_auto_collapse",
    "proc_colors",
    "proc_gradient",
    "proc_per_core",
    "proc_mem_bytes",
    "keep_dead_proc_usage",
    "proc_cpu_graphs",
    "proc_filter_kernel",
    "proc_follow_detailed",
];
const OPTION_CATEGORIES: [&[&str]; 6] = [
    OPTIONS_GENERAL,
    OPTIONS_CPU,
    OPTIONS_GPU,
    OPTIONS_MEM,
    OPTIONS_NET,
    OPTIONS_PROC,
];

pub struct AppState {
    pub config: Config,
    pub sample: Sample,
    pub cpu_history: VecDeque<f64>,
    cpu_watts_max: f64,
    cpu_field_histories: HashMap<String, VecDeque<f64>>,
    core_histories: Vec<VecDeque<f64>>,
    core_temperature_histories: Vec<VecDeque<f64>>,
    pub temp_history: VecDeque<f64>,
    pub mem_history: VecDeque<f64>,
    pub available_history: VecDeque<f64>,
    pub cached_history: VecDeque<f64>,
    pub free_history: VecDeque<f64>,
    swap_used_history: VecDeque<f64>,
    swap_free_history: VecDeque<f64>,
    disk_histories: HashMap<String, DiskHistory>,
    pub download_history: VecDeque<f64>,
    pub upload_history: VecDeque<f64>,
    network_histories: HashMap<String, NetworkHistory>,
    download_top: u64,
    upload_top: u64,
    network_graph_max: [f64; 2],
    network_max_count: [[u8; 2]; 2],
    network_scale_interface: String,
    network_scale_settings: (bool, bool),
    network_raw_totals: HashMap<String, (u64, u64)>,
    network_offsets: HashMap<String, (u64, u64)>,
    network_hitboxes: Vec<NetworkHitbox>,
    cpu_control_hitboxes: Vec<CpuControlHitbox>,
    memory_control_hitboxes: Vec<MemoryControlHitbox>,
    gpu_histories: Vec<GpuHistory>,
    pub selected_process: usize,
    pub process_selected: bool,
    pub process_offset: usize,
    help_page: usize,
    options_selected: usize,
    options_page: usize,
    options_category: usize,
    options_editing: bool,
    options_buffer: String,
    renice_buffer: String,
    signal_buffer: String,
    last_size: Option<Size>,
    pub needs_redraw: bool,
    overlay: Overlay,
    filter_editing: bool,
    filter_buffer: String,
    detailed_pid: Option<u32>,
    detailed_process: Option<ProcessSample>,
    last_selected_process: Option<usize>,
    followed_pid: Option<u32>,
    visible_pids: Vec<u32>,
    collapsed_processes: HashSet<u32>,
    process_tree_active: bool,
    process_hitboxes: Vec<ProcessHitbox>,
    process_control_hitboxes: Vec<ProcessControlHitbox>,
    process_scrollbar: Option<ProcessScrollbar>,
    dragging_process_scrollbar: bool,
    process_cpu_histories: HashMap<u32, VecDeque<f64>>,
    detailed_history_pid: Option<u32>,
    detailed_cpu_history: VecDeque<f64>,
    detailed_memory_history: VecDeque<f64>,
    main_menu_hitboxes: Vec<MainMenuHitbox>,
    signal_confirm_hitboxes: Vec<(bool, Rect)>,
    signal_choice_hitboxes: Vec<(u8, Rect)>,
    cpu_area: Option<Rect>,
    memory_area: Option<Rect>,
    network_area: Option<Rect>,
    process_area: Option<Rect>,
    debug: bool,
    draw_times_us: [u64; 6],
}

#[derive(Debug, Clone, Copy)]
struct ProcessHitbox {
    y: usize,
    index: usize,
    pid: u32,
    toggle_x: Option<(usize, usize)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProcessControlAction {
    Filter,
    DeleteFilter,
    Pause,
    PerCore,
    Reverse,
    Tree,
    SortPrevious,
    SortNext,
    Info,
    Terminate,
    Kill,
    Signals,
    Nice,
    Follow,
}

#[derive(Debug, Clone, Copy)]
struct ProcessControlHitbox {
    y: usize,
    start: usize,
    end: usize,
    action: ProcessControlAction,
}

#[derive(Debug, Clone, Copy)]
struct ProcessScrollbar {
    x: usize,
    up_y: usize,
    down_y: usize,
    track_top: usize,
    track_bottom: usize,
    thumb_y: usize,
    total: usize,
    visible: usize,
}

#[derive(Debug, Clone, Copy)]
struct MainMenuHitbox {
    item: u8,
    x: usize,
    y: usize,
    width: usize,
    height: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum NetworkAction {
    Sync,
    Auto,
    Zero,
    Previous,
    Next,
}

#[derive(Debug, Clone, Copy)]
struct NetworkHitbox {
    y: usize,
    start: usize,
    end: usize,
    action: NetworkAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuControlAction {
    Menu,
    Preset,
    DecreaseUpdate,
    IncreaseUpdate,
}

#[derive(Debug, Clone, Copy)]
struct CpuControlHitbox {
    y: usize,
    start: usize,
    end: usize,
    action: CpuControlAction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemoryControlAction {
    Disks,
    IoMode,
}

#[derive(Debug, Clone, Copy)]
struct MemoryControlHitbox {
    y: usize,
    start: usize,
    end: usize,
    action: MemoryControlAction,
}

#[derive(Default)]
struct GpuHistory {
    utilization: VecDeque<f64>,
    temperature: VecDeque<f64>,
    memory_used: VecDeque<f64>,
    memory_utilization: VecDeque<f64>,
    power: VecDeque<f64>,
}

#[derive(Default)]
struct DiskHistory {
    read: VecDeque<f64>,
    write: VecDeque<f64>,
    activity: VecDeque<f64>,
}

#[derive(Debug, Clone, Default)]
struct NetworkHistory {
    download: VecDeque<f64>,
    upload: VecDeque<f64>,
    download_top: u64,
    upload_top: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Overlay {
    None,
    Main { selected: u8 },
    Help,
    Options,
    Signal { pid: u32, signal: i32 },
    SignalChoose { pid: u32, selected: u8 },
    Renice { pid: u32, value: i32 },
    OperationError { operation: Operation, errno: i32 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Operation {
    Signal,
    Renice,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            filter_buffer: config.process_filter.clone(),
            config,
            sample: Sample::default(),
            cpu_history: VecDeque::new(),
            cpu_watts_max: 0.0,
            cpu_field_histories: HashMap::new(),
            core_histories: Vec::new(),
            core_temperature_histories: Vec::new(),
            temp_history: VecDeque::new(),
            mem_history: VecDeque::new(),
            available_history: VecDeque::new(),
            cached_history: VecDeque::new(),
            free_history: VecDeque::new(),
            swap_used_history: VecDeque::new(),
            swap_free_history: VecDeque::new(),
            disk_histories: HashMap::new(),
            download_history: VecDeque::new(),
            upload_history: VecDeque::new(),
            network_histories: HashMap::new(),
            download_top: 0,
            upload_top: 0,
            network_graph_max: [10_240.0; 2],
            network_max_count: [[0; 2]; 2],
            network_scale_interface: String::new(),
            network_scale_settings: (true, true),
            network_raw_totals: HashMap::new(),
            network_offsets: HashMap::new(),
            network_hitboxes: Vec::new(),
            cpu_control_hitboxes: Vec::new(),
            memory_control_hitboxes: Vec::new(),
            gpu_histories: Vec::new(),
            selected_process: 0,
            process_selected: false,
            process_offset: 0,
            help_page: 0,
            options_selected: 0,
            options_page: 0,
            options_category: 0,
            options_editing: false,
            options_buffer: String::new(),
            renice_buffer: String::new(),
            signal_buffer: String::new(),
            last_size: None,
            needs_redraw: true,
            overlay: Overlay::None,
            filter_editing: false,
            detailed_pid: None,
            detailed_process: None,
            last_selected_process: None,
            followed_pid: None,
            visible_pids: Vec::new(),
            collapsed_processes: HashSet::new(),
            process_tree_active: false,
            process_hitboxes: Vec::new(),
            process_control_hitboxes: Vec::new(),
            process_scrollbar: None,
            dragging_process_scrollbar: false,
            process_cpu_histories: HashMap::new(),
            detailed_history_pid: None,
            detailed_cpu_history: VecDeque::new(),
            detailed_memory_history: VecDeque::new(),
            main_menu_hitboxes: Vec::new(),
            signal_confirm_hitboxes: Vec::new(),
            signal_choice_hitboxes: Vec::new(),
            cpu_area: None,
            memory_area: None,
            network_area: None,
            process_area: None,
            debug: false,
            draw_times_us: [0; 6],
        }
    }

    pub fn update(&mut self, mut sample: Sample) {
        if self.config.process_tree && !self.process_tree_active {
            let threshold = self
                .config
                .value("proc_tree_auto_collapse")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            auto_collapse_oversized(&sample.processes, threshold, &mut self.collapsed_processes);
        }
        self.process_tree_active = self.config.process_tree;
        push(&mut self.cpu_history, sample.cpu.total);
        if let Some(watts) = sample.cpu.watts {
            self.cpu_watts_max = self.cpu_watts_max.max(watts);
        }
        for (name, value) in &sample.cpu.fields {
            push(
                self.cpu_field_histories.entry(name.clone()).or_default(),
                *value,
            );
        }
        self.core_histories
            .resize_with(sample.cpu.cores.len(), VecDeque::new);
        for (history, value) in self.core_histories.iter_mut().zip(&sample.cpu.cores) {
            push(history, *value);
        }
        self.core_temperature_histories
            .resize_with(sample.cpu.core_temperatures.len(), VecDeque::new);
        for (history, temperature) in self
            .core_temperature_histories
            .iter_mut()
            .zip(&sample.cpu.core_temperatures)
        {
            if let Some(temperature) = temperature {
                push(history, *temperature);
            }
        }
        if let Some(temperature) = sample.cpu.temperature {
            push(&mut self.temp_history, temperature);
        }
        let mem = ratio(sample.memory.used, sample.memory.total).round();
        push(&mut self.mem_history, mem);
        push(
            &mut self.available_history,
            ratio(sample.memory.available, sample.memory.total).round(),
        );
        push(
            &mut self.cached_history,
            ratio(sample.memory.cached, sample.memory.total).round(),
        );
        push(
            &mut self.free_history,
            ratio(sample.memory.free, sample.memory.total).round(),
        );
        if sample.memory.swap_total > 0 {
            push(
                &mut self.swap_used_history,
                ratio(sample.memory.swap_used, sample.memory.swap_total).round(),
            );
            push(
                &mut self.swap_free_history,
                ratio(
                    sample
                        .memory
                        .swap_total
                        .saturating_sub(sample.memory.swap_used),
                    sample.memory.swap_total,
                )
                .round(),
            );
        }
        let mounted: HashSet<&str> = sample
            .memory
            .disks
            .iter()
            .map(|disk| disk.mount.as_str())
            .collect();
        self.disk_histories
            .retain(|mount, _| mounted.contains(mount.as_str()));
        for disk in &sample.memory.disks {
            let history = self.disk_histories.entry(disk.mount.clone()).or_default();
            push(&mut history.read, disk.read_per_second as f64);
            push(&mut history.write, disk.write_per_second as f64);
            push(&mut history.activity, disk.io_activity);
        }
        let (download_speed, upload_speed) = if sample.network.connected {
            (
                sample.network.download_per_second,
                sample.network.upload_per_second,
            )
        } else {
            (0, 0)
        };
        let history = self
            .network_histories
            .entry(sample.network.selected.clone())
            .or_default();
        push(&mut history.download, download_speed as f64);
        push(&mut history.upload, upload_speed as f64);
        history.download_top = history.download_top.max(download_speed);
        history.upload_top = history.upload_top.max(upload_speed);
        self.download_history.clone_from(&history.download);
        self.upload_history.clone_from(&history.upload);
        self.download_top = history.download_top;
        self.upload_top = history.upload_top;
        self.update_network_scale(&sample.network);
        self.gpu_histories
            .resize_with(sample.gpus.len(), GpuHistory::default);
        for (gpu, history) in sample.gpus.iter().zip(&mut self.gpu_histories) {
            push(&mut history.utilization, f64::from(gpu.utilization));
            push(&mut history.temperature, gpu.temperature_c as f64);
            push(
                &mut history.memory_used,
                ratio(gpu.memory_used, gpu.memory_total).round(),
            );
            push(
                &mut history.memory_utilization,
                f64::from(gpu.memory_utilization),
            );
            push(&mut history.power, ratio(gpu.power_mw, gpu.power_limit_mw));
        }
        if self.config.pause_processes {
            let current_pids: HashSet<u32> =
                sample.processes.iter().map(|process| process.pid).collect();
            let keep_usage = self
                .config
                .bool_value("keep_dead_proc_usage")
                .unwrap_or(false);
            sample.processes = std::mem::take(&mut self.sample.processes);
            for process in &mut sample.processes {
                if process.state == 'X' || !current_pids.contains(&process.pid) {
                    process.state = 'X';
                    if !keep_usage {
                        process.cpu = 0.0;
                        process.memory = 0;
                    }
                }
            }
            sample.process_count = self.sample.process_count;
        }
        let current_pids: HashSet<u32> =
            sample.processes.iter().map(|process| process.pid).collect();
        self.process_cpu_histories
            .retain(|pid, _| current_pids.contains(pid));
        for process in &sample.processes {
            let graph_value = if (0.1..5.0).contains(&process.cpu) {
                5.0
            } else {
                process.cpu.round()
            };
            push(
                self.process_cpu_histories.entry(process.pid).or_default(),
                graph_value,
            );
        }
        if self.detailed_history_pid != self.detailed_pid {
            self.detailed_history_pid = self.detailed_pid;
            self.detailed_cpu_history.clear();
            self.detailed_memory_history.clear();
        }
        match self.detailed_pid {
            Some(pid) => {
                if let Some(process) = sample.processes.iter().find(|process| process.pid == pid) {
                    self.detailed_process = Some(process.clone());
                } else if let Some(process) = self
                    .detailed_process
                    .as_mut()
                    .filter(|process| process.pid == pid)
                {
                    // btop retains the last detail entry and freezes elapsed
                    // runtime when the selected process disappears.
                    process.state = 'X';
                } else {
                    self.detailed_process = None;
                }
            }
            None => self.detailed_process = None,
        }
        if let Some(pid) = self.detailed_pid
            && let Some(process) = sample.processes.iter().find(|process| process.pid == pid)
        {
            let detailed_cpu = if self.config.process_per_core {
                process.cpu
            } else {
                process.cpu * sample.cpu.cores.len().max(1) as f64
            };
            push(&mut self.detailed_cpu_history, detailed_cpu.min(100.0));
            push(&mut self.detailed_memory_history, process.memory as f64);
        }
        let interface = sample.network.selected.clone();
        let raw_totals = (sample.network.downloaded, sample.network.uploaded);
        self.network_raw_totals
            .insert(interface.clone(), raw_totals);
        if let Some((download_offset, upload_offset)) = self.network_offsets.get(&interface) {
            sample.network.downloaded = sample.network.downloaded.saturating_sub(*download_offset);
            sample.network.uploaded = sample.network.uploaded.saturating_sub(*upload_offset);
        }
        self.sample = sample;
    }

    pub fn set_debug(&mut self, enabled: bool) {
        self.debug = enabled;
    }

    fn update_network_scale(&mut self, network: &NetworkSample) {
        let settings = (self.config.net_auto, self.config.net_sync);
        let rescale = self.network_scale_interface != network.selected
            || self.network_scale_settings != settings;
        if rescale {
            self.network_scale_interface.clone_from(&network.selected);
            self.network_scale_settings = settings;
            self.network_max_count = [[0; 2]; 2];
        }
        if !self.config.net_auto {
            return;
        }
        let speeds = [
            network.download_per_second as f64,
            network.upload_per_second as f64,
        ];
        for direction in 0..2 {
            if self.config.net_sync && speeds[direction] < speeds[1 - direction] {
                continue;
            }
            if speeds[direction] > self.network_graph_max[direction] {
                self.network_max_count[direction][0] =
                    self.network_max_count[direction][0].saturating_add(1);
                self.network_max_count[direction][1] =
                    self.network_max_count[direction][1].saturating_sub(1);
            } else if self.network_graph_max[direction] > 10_240.0
                && speeds[direction] < self.network_graph_max[direction] / 10.0
            {
                self.network_max_count[direction][1] =
                    self.network_max_count[direction][1].saturating_add(1);
                self.network_max_count[direction][0] =
                    self.network_max_count[direction][0].saturating_sub(1);
            }
            let scale_down = self.network_max_count[direction][1] >= 5;
            if rescale || self.network_max_count[direction][0] >= 5 || scale_down {
                let history = if direction == 0 {
                    &self.download_history
                } else {
                    &self.upload_history
                };
                let average = recent_average(history, 5).unwrap_or(speeds[direction]);
                let maximum = (average * if scale_down { 3.0 } else { 1.3 }).max(10_240.0);
                self.network_graph_max[direction] = maximum;
                self.network_max_count[direction] = [0; 2];
                if self.config.net_sync {
                    self.network_graph_max[1 - direction] = maximum;
                    self.network_max_count[1 - direction] = [0; 2];
                    break;
                }
            }
        }
    }

    pub fn should_collect(&self) -> bool {
        self.overlay == Overlay::None || self.config.bool_value("background_update").unwrap_or(true)
    }

    pub fn detailed_pid(&self) -> Option<u32> {
        self.detailed_pid
    }

    pub fn handle_key(&mut self, key: Key) -> bool {
        if self.filter_editing {
            match key {
                Key::Enter => {
                    self.config.process_filter.clone_from(&self.filter_buffer);
                    self.filter_editing = false;
                }
                Key::Escape => {
                    self.filter_buffer.clone_from(&self.config.process_filter);
                    self.filter_editing = false;
                }
                Key::Backspace => {
                    self.filter_buffer.pop();
                }
                Key::Delete => self.filter_buffer.clear(),
                Key::Char(ch) if !ch.is_control() => self.filter_buffer.push(ch),
                _ => {}
            }
            self.needs_redraw = true;
            return false;
        }
        if let Overlay::OperationError { .. } = self.overlay {
            let clicked_ok = match (key, self.last_size) {
                (
                    Key::Mouse {
                        button: 0,
                        x,
                        y,
                        pressed: true,
                    },
                    Some(size),
                ) => {
                    let width = 50.min(size.cols.saturating_sub(4) as usize);
                    let height = 9.min(size.rows.saturating_sub(2) as usize);
                    let area = Rect::new(
                        source_center_x(size.cols as usize, width),
                        (size.rows as usize).saturating_sub(height) / 2,
                        width,
                        height,
                    );
                    Rect::new(area.x + width / 2 - 5, area.y + height - 4, 12, 3)
                        .contains(x as usize, y as usize)
                }
                _ => false,
            };
            if matches!(key, Key::Enter | Key::Escape | Key::Char(' ' | 'q')) || clicked_ok {
                self.overlay = Overlay::None;
                self.needs_redraw = true;
            }
            return false;
        }
        if let Overlay::Signal { pid, signal } = self.overlay {
            match key {
                Key::Enter | Key::Char('y') | Key::Char('Y') => {
                    self.overlay = operation_result(Operation::Signal, send_signal(pid, signal));
                }
                Key::Escape | Key::Char('n') | Key::Char('N') | Key::Char('q') => {
                    self.overlay = Overlay::None;
                }
                Key::Mouse {
                    button: 0,
                    x,
                    y,
                    pressed: true,
                } => {
                    if let Some(confirm) = self
                        .signal_confirm_hitboxes
                        .iter()
                        .find(|(_, area)| area.contains(x as usize, y as usize))
                        .map(|(confirm, _)| *confirm)
                    {
                        if confirm {
                            self.overlay =
                                operation_result(Operation::Signal, send_signal(pid, signal));
                        } else {
                            self.overlay = Overlay::None;
                        }
                    }
                }
                _ => {}
            }
            self.needs_redraw = true;
            return false;
        }
        if let Overlay::SignalChoose { pid, mut selected } = self.overlay {
            match key {
                Key::Escape | Key::Char('q') => self.overlay = Overlay::None,
                Key::Down | Key::Char('j') => {
                    self.signal_buffer.clear();
                    selected = move_signal_vertical(selected, true);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Up | Key::Char('k') => {
                    self.signal_buffer.clear();
                    selected = move_signal_vertical(selected, false);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Right | Key::Char('l') => {
                    self.signal_buffer.clear();
                    selected = move_signal_horizontal(selected, true);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Left | Key::Char('h') => {
                    self.signal_buffer.clear();
                    selected = move_signal_horizontal(selected, false);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Backspace => {
                    self.signal_buffer.pop();
                    selected = self.signal_buffer.parse().unwrap_or(0);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Char(ch) if ch.is_ascii_digit() => {
                    self.signal_buffer.push(ch);
                    selected = self.signal_buffer.parse::<u8>().unwrap_or(0).min(64);
                    self.overlay = Overlay::SignalChoose { pid, selected };
                }
                Key::Enter | Key::Char(' ') if selected > 0 => {
                    self.overlay =
                        operation_result(Operation::Signal, send_signal(pid, selected as i32));
                }
                Key::Mouse {
                    button: 0,
                    x,
                    y,
                    pressed: true,
                } => {
                    if let Some(signal) = self
                        .signal_choice_hitboxes
                        .iter()
                        .find(|(_, area)| area.contains(x as usize, y as usize))
                        .map(|(signal, _)| *signal)
                    {
                        if signal == selected {
                            self.overlay = operation_result(
                                Operation::Signal,
                                send_signal(pid, signal as i32),
                            );
                        } else {
                            self.overlay = Overlay::SignalChoose {
                                pid,
                                selected: signal,
                            };
                        }
                    }
                }
                _ => return false,
            }
            self.needs_redraw = true;
            return false;
        }
        if let Overlay::Renice { pid, mut value } = self.overlay {
            match key {
                Key::Escape | Key::Char('q' | 'N') => self.overlay = Overlay::None,
                Key::Up | Key::Char('k' | '+') => {
                    value += 1;
                    if value > 19 {
                        value = -20;
                    }
                    self.renice_buffer.clear();
                    self.overlay = Overlay::Renice { pid, value };
                }
                Key::Down | Key::Char('j') => {
                    value -= 1;
                    if value < -20 {
                        value = 19;
                    }
                    self.renice_buffer.clear();
                    self.overlay = Overlay::Renice { pid, value };
                }
                Key::Left | Key::Char('h') => {
                    value -= 5;
                    if value < -20 {
                        value += 40;
                    }
                    self.renice_buffer.clear();
                    self.overlay = Overlay::Renice { pid, value };
                }
                Key::Right | Key::Char('l') => {
                    value += 5;
                    if value > 19 {
                        value -= 40;
                    }
                    self.renice_buffer.clear();
                    self.overlay = Overlay::Renice { pid, value };
                }
                Key::Backspace => {
                    self.renice_buffer.pop();
                    if let Ok(entered) = self.renice_buffer.parse() {
                        value = entered;
                        self.overlay = Overlay::Renice { pid, value };
                    }
                }
                Key::Char(ch)
                    if ch.is_ascii_digit() || (ch == '-' && self.renice_buffer.is_empty()) =>
                {
                    self.renice_buffer.push(ch);
                    if let Ok(entered) = self.renice_buffer.parse() {
                        value = entered;
                        self.overlay = Overlay::Renice { pid, value };
                    }
                }
                Key::Enter | Key::Char(' ') => {
                    let entered = self.renice_buffer.parse().unwrap_or(value);
                    self.overlay = operation_result(Operation::Renice, set_nice(pid, entered));
                }
                _ => return false,
            }
            self.needs_redraw = true;
            return false;
        }
        if let Overlay::Main { mut selected } = self.overlay {
            match key {
                Key::Escape | Key::Char('q') | Key::Char('m') => self.overlay = Overlay::None,
                Key::Mouse {
                    button: 0,
                    x,
                    y,
                    pressed: true,
                } => {
                    let clicked = self.main_menu_hitboxes.iter().find(|hitbox| {
                        (hitbox.x..hitbox.x + hitbox.width).contains(&(x as usize))
                            && (hitbox.y..hitbox.y + hitbox.height).contains(&(y as usize))
                    });
                    if let Some(item) = clicked.map(|hitbox| hitbox.item) {
                        if item == selected {
                            return self.activate_main_menu_item(item);
                        }
                        selected = item;
                        self.overlay = Overlay::Main { selected };
                    } else {
                        self.overlay = Overlay::None;
                    }
                }
                Key::Down | Key::Char('j') => {
                    selected = (selected + 1) % 3;
                    self.overlay = Overlay::Main { selected };
                }
                Key::Up | Key::Char('k') => {
                    selected = (selected + 2) % 3;
                    self.overlay = Overlay::Main { selected };
                }
                Key::Enter | Key::Char(' ') => return self.activate_main_menu_item(selected),
                _ => return false,
            }
            self.needs_redraw = true;
            return false;
        }
        if self.overlay == Overlay::Help {
            match key {
                Key::Escape | Key::Enter | Key::Backspace | Key::Char('q' | 'h' | ' ') => {
                    self.overlay = Overlay::None
                }
                Key::Down | Key::PageDown | Key::Char('j') => {
                    self.help_page = (self.help_page + 1) % 2
                }
                Key::Up | Key::PageUp | Key::Char('k') => self.help_page = (self.help_page + 1) % 2,
                Key::Mouse { button: 64, .. } | Key::Mouse { button: 65, .. } => {
                    self.help_page = (self.help_page + 1) % 2
                }
                Key::Mouse {
                    button: 0,
                    pressed: true,
                    ..
                } => self.overlay = Overlay::None,
                _ => return false,
            }
            self.needs_redraw = true;
            return false;
        }
        if self.overlay == Overlay::Options {
            if self.options_editing {
                match key {
                    Key::Escape => {
                        self.options_editing = false;
                        self.options_buffer.clear();
                    }
                    Key::Enter => self.commit_option_edit(),
                    Key::Backspace => {
                        self.options_buffer.pop();
                    }
                    Key::Delete => self.options_buffer.clear(),
                    Key::Char(ch) if !ch.is_control() => self.options_buffer.push(ch),
                    _ => return false,
                }
                self.needs_redraw = true;
                return false;
            }
            match key {
                Key::Mouse {
                    button,
                    x,
                    y,
                    pressed,
                } => self.handle_options_mouse(button, x as usize, y as usize, pressed),
                Key::Escape | Key::Backspace | Key::Char('q' | 'o') => self.overlay = Overlay::None,
                Key::Down => self.move_option(1),
                Key::Char('j') if self.config.vim_keys => self.move_option(1),
                Key::Up => self.move_option(-1),
                Key::Char('k') if self.config.vim_keys => self.move_option(-1),
                Key::PageDown => self.move_option_page(1),
                Key::PageUp => self.move_option_page(-1),
                Key::Tab => self.move_option_category(1),
                Key::BackTab => self.move_option_category(-1),
                Key::Char('1'..='6') => {
                    if let Key::Char(category) = key {
                        self.options_category = category as usize - '1' as usize;
                        self.options_page = 0;
                        self.options_selected = 0;
                    }
                }
                Key::Right => self.change_option(1),
                Key::Char('l') if self.config.vim_keys => self.change_option(1),
                Key::Left => self.change_option(-1),
                Key::Char('h') if self.config.vim_keys => self.change_option(-1),
                Key::Enter | Key::Char('e' | 'E') => self.activate_option(),
                _ => return false,
            }
            self.needs_redraw = true;
            return false;
        }
        if self.overlay != Overlay::None {
            if matches!(
                key,
                Key::Escape
                    | Key::Enter
                    | Key::Char('q')
                    | Key::Char('m')
                    | Key::Char('?')
                    | Key::Char('h')
                    | Key::Char('o')
            ) {
                self.overlay = Overlay::None;
                self.needs_redraw = true;
            }
            return false;
        }
        match key {
            Key::CtrlC | Key::Char('q') => return true,
            Key::Escape | Key::Char('m') => self.activate_cpu_control(CpuControlAction::Menu),
            Key::F1 | Key::Char('?') | Key::Char('h') => {
                self.help_page = 0;
                self.overlay = Overlay::Help;
            }
            Key::F2 | Key::Char('o') => {
                self.options_selected = 0;
                self.options_page = 0;
                self.options_category = 0;
                self.overlay = Overlay::Options;
            }
            Key::Char('1'..='4') => {
                if let Key::Char(ch) = key {
                    let name = ["cpu", "mem", "net", "proc"][ch as usize - '1' as usize];
                    self.config.toggle_box(name);
                    self.config.preset = None;
                }
            }
            Key::Char('0') if self.sample.gpus.len() >= 6 => {
                self.config.toggle_box("gpu5");
                self.config.preset = None;
            }
            Key::Char('5'..='9') => {
                if let Key::Char(ch) = key {
                    let index = ch as usize - '5' as usize;
                    if index < self.sample.gpus.len() {
                        self.config.toggle_box(&format!("gpu{index}"));
                        self.config.preset = None;
                    }
                }
            }
            Key::Char('p') => self.activate_cpu_control(CpuControlAction::Preset),
            Key::Char('P') => {
                self.config.cycle_preset(false);
            }
            Key::Char('+') | Key::Char('=')
                if self.config.process_tree && self.process_selected =>
            {
                self.set_selected_process_collapsed(false)
            }
            Key::Char('-') if self.config.process_tree && self.process_selected => {
                self.set_selected_process_collapsed(true)
            }
            Key::Char(' ') if self.config.process_tree && self.process_selected => {
                self.toggle_selected_process_collapsed()
            }
            Key::Char('C') if self.config.process_tree && self.process_selected => {
                self.toggle_selected_process_children()
            }
            Key::Char('E') if self.config.process_tree => self.toggle_all_process_collapse(),
            Key::Char('+') | Key::Char('=') => {
                self.activate_cpu_control(CpuControlAction::IncreaseUpdate)
            }
            Key::Char('-') => self.activate_cpu_control(CpuControlAction::DecreaseUpdate),
            Key::Left => self.activate_process_control(ProcessControlAction::SortPrevious),
            Key::Right => self.activate_process_control(ProcessControlAction::SortNext),
            Key::Enter => self.toggle_process_details(),
            Key::Up => self.select_up(1),
            Key::Char('k') if self.config.vim_keys => self.select_up(1),
            Key::Down => self.select_down(1),
            Key::Char('j') if self.config.vim_keys => self.select_down(1),
            Key::PageUp => self.page_up(),
            Key::PageDown => self.page_down(),
            Key::Home | Key::Char('g') if key == Key::Home || self.config.vim_keys => {
                if !self.config.pause_processes {
                    self.followed_pid = None;
                }
                self.selected_process = 0;
                self.process_offset = 0;
                self.process_selected = true;
            }
            Key::End | Key::Char('G') if key == Key::End || self.config.vim_keys => {
                if !self.config.pause_processes {
                    self.followed_pid = None;
                }
                self.selected_process = usize::MAX;
                self.process_selected = true;
            }
            Key::Char('r') => self.activate_process_control(ProcessControlAction::Reverse),
            Key::Char('e') => self.activate_process_control(ProcessControlAction::Tree),
            Key::Char('u') => self.activate_process_control(ProcessControlAction::Pause),
            Key::Char('t') => self.activate_process_control(ProcessControlAction::Terminate),
            Key::Char('k') if !self.config.vim_keys => {
                self.activate_process_control(ProcessControlAction::Kill)
            }
            Key::Char('K') if self.config.vim_keys => {
                self.activate_process_control(ProcessControlAction::Kill)
            }
            Key::Char('s') => self.activate_process_control(ProcessControlAction::Signals),
            Key::Char('N') => self.activate_process_control(ProcessControlAction::Nice),
            Key::Char('F') => self.activate_process_control(ProcessControlAction::Follow),
            Key::Char('c') => self.activate_process_control(ProcessControlAction::PerCore),
            Key::Char('%') => self.config.process_mem_bytes ^= true,
            Key::Char('/') | Key::Char('f') => {
                self.activate_process_control(ProcessControlAction::Filter)
            }
            Key::Delete => self.config.process_filter.clear(),
            Key::Char('d') => self.activate_memory_control(MemoryControlAction::Disks),
            Key::Char('i') if self.config.show_disks => {
                self.activate_memory_control(MemoryControlAction::IoMode)
            }
            Key::Char('a') => self.activate_network_control(NetworkAction::Auto),
            Key::Char('y') => self.activate_network_control(NetworkAction::Sync),
            Key::Char('z') => self.activate_network_control(NetworkAction::Zero),
            Key::Char('n') => self.activate_network_control(NetworkAction::Next),
            Key::Char('b') => self.activate_network_control(NetworkAction::Previous),
            Key::Mouse {
                button,
                x,
                y,
                pressed,
            } => return self.handle_mouse(button, x as usize, y as usize, pressed),
            _ => return false,
        }
        self.needs_redraw = true;
        false
    }

    fn activate_main_menu_item(&mut self, selected: u8) -> bool {
        match selected {
            0 => {
                self.options_selected = 0;
                self.options_page = 0;
                self.options_category = 0;
                self.overlay = Overlay::Options;
                false
            }
            1 => {
                self.help_page = 0;
                self.overlay = Overlay::Help;
                false
            }
            _ => true,
        }
    }

    fn rotate_sort(&mut self, forward: bool) {
        let index = ProcessSort::ALL
            .iter()
            .position(|sort| *sort == self.config.process_sort)
            .unwrap_or(0);
        let next = if forward {
            (index + 1) % ProcessSort::ALL.len()
        } else {
            (index + ProcessSort::ALL.len() - 1) % ProcessSort::ALL.len()
        };
        self.config.process_sort = ProcessSort::ALL[next];
    }

    fn options_per_page(&self) -> usize {
        let rows = self.last_size.map(|size| size.rows as usize).unwrap_or(40);
        let max_items = OPTION_CATEGORIES
            .iter()
            .map(|category| category.len())
            .max()
            .unwrap_or(1);
        let height = rows.saturating_sub(7).min(max_items * 2 + 4) & !1;
        (height.saturating_sub(4) / 2).max(1)
    }

    fn options_area(&self) -> Option<Rect> {
        let size = self.last_size?;
        let width = 78.min((size.cols as usize).saturating_sub(2));
        let max_items = OPTION_CATEGORIES
            .iter()
            .map(|category| category.len())
            .max()
            .unwrap_or(1);
        let height = (size.rows as usize)
            .saturating_sub(7)
            .min(max_items * 2 + 4)
            & !1;
        let banner_y = (size.rows as usize)
            .saturating_div(2)
            .saturating_sub(4 + max_items);
        Some(Rect::new(
            source_center_x(size.cols as usize, width),
            banner_y + 6,
            width,
            height,
        ))
    }

    fn handle_options_mouse(&mut self, button: u16, x: usize, y: usize, pressed: bool) {
        if button == 64 {
            self.move_option(-1);
            return;
        }
        if button == 65 {
            self.move_option(1);
            return;
        }
        if !pressed || button != 0 {
            return;
        }
        let Some(area) = self.options_area() else {
            return;
        };
        if x < area.x || x >= area.x + area.w || y < area.y || y >= area.y + area.h {
            self.overlay = Overlay::None;
            return;
        }
        if (area.y..=area.y + 2).contains(&y) {
            for category in 0..OPTION_CATEGORIES.len() {
                let start = area.x + 2 + category * 12;
                if (start..start + 11).contains(&x) {
                    self.options_category = category;
                    self.options_page = 0;
                    self.options_selected = 0;
                    return;
                }
            }
        }
        if x < area.x + 30 && y >= area.y + 3 && y < area.y + area.h - 1 {
            let selected = (y - area.y - 3) / 2;
            let option_index = self.options_page * self.options_per_page() + selected;
            if option_index < OPTION_CATEGORIES[self.options_category].len() {
                if self.options_selected == selected {
                    if x < area.x + 6 {
                        self.change_option(-1);
                    } else if x >= area.x + 25 {
                        self.change_option(1);
                    } else if self
                        .current_option()
                        .is_some_and(|option| option_is_editable(option, self))
                    {
                        self.activate_option();
                    }
                } else {
                    self.options_selected = selected;
                }
            }
        }
    }

    fn current_option(&self) -> Option<&'static str> {
        let options = OPTION_CATEGORIES.get(self.options_category)?;
        options
            .get(self.options_page * self.options_per_page() + self.options_selected)
            .copied()
    }

    fn move_option(&mut self, direction: i32) {
        let options = OPTION_CATEGORIES[self.options_category];
        if options.is_empty() {
            return;
        }
        let per_page = self.options_per_page();
        let current = (self.options_page * per_page + self.options_selected).min(options.len() - 1);
        let next = if direction > 0 {
            (current + 1) % options.len()
        } else {
            (current + options.len() - 1) % options.len()
        };
        self.options_page = next / per_page;
        self.options_selected = next % per_page;
    }

    fn move_option_page(&mut self, direction: i32) {
        let options = OPTION_CATEGORIES[self.options_category];
        let pages = options.len().div_ceil(self.options_per_page()).max(1);
        self.options_page = if direction > 0 {
            (self.options_page + 1) % pages
        } else {
            (self.options_page + pages - 1) % pages
        };
        self.options_selected = 0;
    }

    fn move_option_category(&mut self, direction: i32) {
        self.options_category = if direction > 0 {
            (self.options_category + 1) % OPTION_CATEGORIES.len()
        } else {
            (self.options_category + OPTION_CATEGORIES.len() - 1) % OPTION_CATEGORIES.len()
        };
        self.options_page = 0;
        self.options_selected = 0;
    }

    fn change_option(&mut self, direction: i32) {
        let Some(option) = self.current_option() else {
            return;
        };
        if self.config.bool_value(option).is_some() {
            self.config.flip_value(option);
            return;
        }
        if is_integer_option(option) {
            let step = if option == "update_ms" { 100 } else { 1 };
            let current = self
                .config
                .value(option)
                .and_then(|value| value.parse::<i64>().ok())
                .unwrap_or(0);
            let mut next = current + i64::from(direction) * step;
            if option == "update_ms" {
                next = next.clamp(100, 86_400_000);
            } else if option == "proc_tree_auto_collapse" {
                next = next.clamp(0, 10_000);
            }
            self.config.set_value(option, next.to_string());
            return;
        }
        if option == "color_theme" {
            let choices = theme_choices(&self.config);
            cycle_theme(&mut self.config, &choices, direction);
        } else if let Some(choices) = dynamic_option_choices(option, self) {
            cycle_option(&mut self.config, option, &choices, direction);
        } else if let Some(choices) = option_choices(option) {
            cycle_option(&mut self.config, option, choices, direction);
        }
    }

    fn activate_option(&mut self) {
        let Some(option) = self.current_option() else {
            return;
        };
        if self.config.bool_value(option).is_some()
            || option == "color_theme"
            || dynamic_option_choices(option, self).is_some()
            || option_choices(option).is_some()
        {
            self.change_option(1);
        } else {
            self.options_buffer = self.config.value(option).unwrap_or_default().to_string();
            self.options_editing = true;
        }
    }

    fn commit_option_edit(&mut self) {
        let Some(option) = self.current_option() else {
            self.options_editing = false;
            return;
        };
        let valid = if is_integer_option(option) {
            self.options_buffer.parse::<i64>().is_ok_and(|value| {
                (option != "update_ms" || (100..=86_400_000).contains(&value))
                    && (option != "proc_tree_auto_collapse" || (0..=10_000).contains(&value))
            })
        } else if option == "shown_boxes" {
            let boxes: Vec<&str> = self.options_buffer.split_whitespace().collect();
            !boxes.is_empty()
                && boxes.iter().all(|name| {
                    matches!(*name, "cpu" | "mem" | "net" | "proc")
                        || name
                            .strip_prefix("gpu")
                            .and_then(|index| index.parse::<usize>().ok())
                            .is_some_and(|index| index < self.sample.gpus.len())
                })
        } else {
            true
        };
        if valid {
            self.config.set_value(option, self.options_buffer.clone());
            self.options_editing = false;
            self.options_buffer.clear();
        }
    }
    fn select_up(&mut self, amount: usize) {
        self.restore_followed_selection();
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
        if self.process_selected && self.selected_process < amount {
            self.selected_process = 0;
            self.process_selected = false;
        } else if self.process_selected {
            self.selected_process = self.selected_process.saturating_sub(amount);
        }
    }
    fn select_down(&mut self, amount: usize) {
        self.restore_followed_selection();
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
        if self.process_selected {
            self.selected_process = self.selected_process.saturating_add(amount);
        } else {
            self.selected_process = amount.saturating_sub(1);
            self.process_selected = true;
        }
    }

    fn restore_followed_selection(&mut self) {
        if !self.process_selected
            && let Some(pid) = self.followed_pid
            && let Some(index) = self
                .visible_pids
                .iter()
                .position(|candidate| *candidate == pid)
        {
            self.selected_process = index;
            self.process_selected = true;
        }
    }

    fn process_page_rows(&self) -> usize {
        self.process_scrollbar
            .map(|scrollbar| scrollbar.visible)
            .or_else(|| {
                self.process_area.map(|area| {
                    area.h.saturating_sub(
                        4 + usize::from(area.h >= 14 && self.detailed_pid.is_some()) * 8,
                    )
                })
            })
            .unwrap_or(10)
            .max(1)
    }

    fn page_up(&mut self) {
        self.restore_followed_selection();
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
        if self.process_selected && self.process_offset == 0 {
            self.process_selected = false;
            return;
        }
        if self.process_selected {
            let row = self.selected_process.saturating_sub(self.process_offset);
            self.process_offset = self.process_offset.saturating_sub(self.process_page_rows());
            self.selected_process = self.process_offset.saturating_add(row);
        }
    }

    fn page_down(&mut self) {
        self.restore_followed_selection();
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
        let rows = self.process_page_rows();
        let maximum = self.visible_pids.len().saturating_sub(rows);
        if self.process_selected {
            let row = self.selected_process.saturating_sub(self.process_offset);
            if self.process_offset >= maximum {
                self.selected_process = self.visible_pids.len().saturating_sub(1);
            } else {
                self.process_offset = self.process_offset.saturating_add(rows).min(maximum);
                self.selected_process = self
                    .process_offset
                    .saturating_add(row)
                    .min(self.visible_pids.len().saturating_sub(1));
            }
        } else if !self.visible_pids.is_empty() {
            self.selected_process = self.process_offset;
            self.process_selected = true;
        }
    }

    fn scroll_processes(&mut self, down: bool, amount: usize) {
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
        let maximum = self
            .visible_pids
            .len()
            .saturating_sub(self.process_page_rows());
        let previous = self.process_offset;
        self.process_offset = if down {
            self.process_offset.saturating_add(amount).min(maximum)
        } else {
            self.process_offset.saturating_sub(amount)
        };
        if self.process_selected {
            let moved = self.process_offset.abs_diff(previous);
            self.selected_process = if down {
                self.selected_process
                    .saturating_add(moved)
                    .min(self.visible_pids.len().saturating_sub(1))
            } else {
                self.selected_process.saturating_sub(moved)
            };
        }
    }
    fn cycle_interface(&mut self, forward: bool) {
        let list = &self.sample.network.interfaces;
        if list.is_empty() {
            return;
        }
        let current = list
            .iter()
            .position(|v| v == &self.sample.network.selected)
            .unwrap_or(0);
        let next = if forward {
            (current + 1) % list.len()
        } else {
            (current + list.len() - 1) % list.len()
        };
        self.config.net_iface = Some(list[next].clone());
    }

    fn network_zero_active(&self) -> bool {
        self.network_offsets
            .get(&self.sample.network.selected)
            .is_some_and(|(download, upload)| download.saturating_add(*upload) > 0)
    }

    fn toggle_network_zero(&mut self) {
        let interface = self.sample.network.selected.clone();
        if interface.is_empty() {
            return;
        }
        if self.network_zero_active() {
            self.network_offsets.remove(&interface);
            if let Some((downloaded, uploaded)) = self.network_raw_totals.get(&interface) {
                self.sample.network.downloaded = *downloaded;
                self.sample.network.uploaded = *uploaded;
            }
        } else {
            let totals = self
                .network_raw_totals
                .get(&interface)
                .copied()
                .unwrap_or((self.sample.network.downloaded, self.sample.network.uploaded));
            self.network_raw_totals.insert(interface.clone(), totals);
            self.network_offsets.insert(interface, totals);
            self.sample.network.downloaded = 0;
            self.sample.network.uploaded = 0;
        }
    }

    fn activate_network_control(&mut self, action: NetworkAction) {
        match action {
            NetworkAction::Sync => self.config.net_sync ^= true,
            NetworkAction::Auto => self.config.net_auto ^= true,
            NetworkAction::Zero => self.toggle_network_zero(),
            NetworkAction::Previous => self.cycle_interface(false),
            NetworkAction::Next => self.cycle_interface(true),
        }
    }

    fn activate_process_control(&mut self, action: ProcessControlAction) {
        match action {
            ProcessControlAction::Filter => {
                self.filter_editing = true;
                self.filter_buffer.clone_from(&self.config.process_filter);
            }
            ProcessControlAction::DeleteFilter => {
                self.config.process_filter.clear();
                self.filter_buffer.clear();
            }
            ProcessControlAction::Pause => self.config.pause_processes ^= true,
            ProcessControlAction::PerCore => self.config.process_per_core ^= true,
            ProcessControlAction::Reverse => self.config.process_reversed ^= true,
            ProcessControlAction::Tree => self.config.process_tree ^= true,
            ProcessControlAction::SortPrevious => self.rotate_sort(false),
            ProcessControlAction::SortNext => self.rotate_sort(true),
            ProcessControlAction::Info => self.toggle_process_details(),
            ProcessControlAction::Terminate => self.open_signal(15),
            ProcessControlAction::Kill => self.open_signal(9),
            ProcessControlAction::Signals => self.open_signal_chooser(),
            ProcessControlAction::Nice => self.open_renice(),
            ProcessControlAction::Follow => self.toggle_follow(),
        }
    }

    fn activate_cpu_control(&mut self, action: CpuControlAction) {
        match action {
            CpuControlAction::Menu => self.overlay = Overlay::Main { selected: 0 },
            CpuControlAction::Preset => self.config.cycle_preset(true),
            CpuControlAction::DecreaseUpdate => {
                self.config.update_ms = self.config.update_ms.saturating_sub(100).max(100)
            }
            CpuControlAction::IncreaseUpdate => {
                self.config.update_ms = (self.config.update_ms + 100).min(86_400_000)
            }
        }
    }

    fn activate_memory_control(&mut self, action: MemoryControlAction) {
        match action {
            MemoryControlAction::Disks => self.config.show_disks ^= true,
            MemoryControlAction::IoMode => self.config.flip_value("io_mode"),
        }
    }

    fn open_signal(&mut self, signal: i32) {
        let pid = self
            .process_selected
            .then(|| self.visible_pids.get(self.selected_process).copied())
            .flatten()
            .or(self.detailed_pid);
        if let Some(pid) = pid {
            self.overlay = Overlay::Signal { pid, signal };
        }
    }

    fn selected_pid(&self) -> Option<u32> {
        self.process_selected
            .then(|| self.visible_pids.get(self.selected_process).copied())
            .flatten()
            .or(self.detailed_pid)
    }

    fn set_selected_process_collapsed(&mut self, collapsed: bool) {
        if let Some(pid) = self.selected_pid() {
            if collapsed {
                self.collapsed_processes.insert(pid);
            } else {
                self.collapsed_processes.remove(&pid);
            }
        }
    }

    fn toggle_selected_process_collapsed(&mut self) {
        if let Some(pid) = self.selected_pid() {
            self.toggle_process_collapsed(pid);
        }
    }

    fn toggle_process_collapsed(&mut self, pid: u32) {
        if !self.collapsed_processes.remove(&pid) {
            self.collapsed_processes.insert(pid);
        }
    }

    fn toggle_selected_process_children(&mut self) {
        let Some(pid) = self.selected_pid() else {
            return;
        };
        let child_pids: Vec<u32> = self
            .sample
            .processes
            .iter()
            .filter(|process| process.parent == pid && process.pid != pid)
            .map(|process| process.pid)
            .collect();
        for child_pid in child_pids {
            if !self.collapsed_processes.remove(&child_pid) {
                self.collapsed_processes.insert(child_pid);
            }
        }
    }

    fn toggle_all_process_collapse(&mut self) {
        let pids: HashSet<u32> = self
            .sample
            .processes
            .iter()
            .map(|process| process.pid)
            .collect();
        let parent_pids: HashSet<u32> = self
            .sample
            .processes
            .iter()
            .map(|process| process.parent)
            .collect();
        let collapse = self.sample.processes.iter().any(|process| {
            parent_pids.contains(&process.pid)
                && pids.contains(&process.parent)
                && !self.collapsed_processes.contains(&process.pid)
        });
        for process in &self.sample.processes {
            if pids.contains(&process.parent) {
                if collapse {
                    self.collapsed_processes.insert(process.pid);
                } else {
                    self.collapsed_processes.remove(&process.pid);
                }
            }
        }
    }

    fn open_signal_chooser(&mut self) {
        if let Some(pid) = self.selected_pid() {
            self.signal_buffer.clear();
            self.overlay = Overlay::SignalChoose { pid, selected: 0 };
        }
    }

    fn open_renice(&mut self) {
        if let Some(pid) = self.selected_pid() {
            self.renice_buffer.clear();
            self.overlay = Overlay::Renice { pid, value: 0 };
        }
    }

    fn toggle_follow(&mut self) {
        let selected = self.selected_pid();
        if selected.is_some() && selected != self.followed_pid {
            self.followed_pid = selected;
        } else if self.followed_pid.is_some() {
            self.followed_pid = None;
        }
    }

    fn toggle_process_details(&mut self) {
        if self.process_selected
            && let Some(pid) = self.visible_pids.get(self.selected_process).copied()
        {
            if self.detailed_pid == Some(pid) {
                self.detailed_pid = None;
                if self.followed_pid == Some(pid) {
                    self.followed_pid = None;
                }
            } else {
                self.last_selected_process = Some(self.selected_process);
                self.detailed_pid = Some(pid);
                self.process_selected = false;
                if self
                    .config
                    .bool_value("proc_follow_detailed")
                    .unwrap_or(false)
                {
                    self.followed_pid = Some(pid);
                }
            }
        } else if self.detailed_pid.is_some() {
            let followed_detail = self.followed_pid == self.detailed_pid;
            if !followed_detail && let Some(selected) = self.last_selected_process.take() {
                self.selected_process = selected;
                self.process_selected = true;
            }
            if followed_detail {
                self.followed_pid = None;
            }
            self.detailed_pid = None;
        }
    }

    fn handle_mouse(&mut self, button: u16, x: usize, y: usize, pressed: bool) -> bool {
        if self.last_size.is_none() {
            return false;
        }
        if !pressed {
            self.dragging_process_scrollbar = false;
            return false;
        }
        if self.dragging_process_scrollbar && button & 32 != 0 {
            self.select_from_process_scrollbar(y);
            self.needs_redraw = true;
            return false;
        }
        if button == 0
            && let Some(scrollbar) = self.process_scrollbar
            && x == scrollbar.x
            && (scrollbar.up_y..=scrollbar.down_y).contains(&y)
        {
            if y == scrollbar.up_y {
                self.page_up();
            } else if y == scrollbar.down_y {
                self.page_down();
            } else {
                self.dragging_process_scrollbar = y == scrollbar.thumb_y;
                self.select_from_process_scrollbar(y);
            }
            self.needs_redraw = true;
            return false;
        }
        if matches!(button, 64 | 65) && self.process_area.is_some_and(|area| area.contains(x, y)) {
            self.scroll_processes(button == 65, 3);
            self.needs_redraw = true;
            return false;
        }
        if button == 0 {
            if let Some(action) = self
                .cpu_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.y == y && (hitbox.start..hitbox.end).contains(&x))
                .map(|hitbox| hitbox.action)
            {
                self.activate_cpu_control(action);
                self.needs_redraw = true;
                return false;
            }
            if let Some(action) = self
                .memory_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.y == y && (hitbox.start..hitbox.end).contains(&x))
                .map(|hitbox| hitbox.action)
            {
                self.activate_memory_control(action);
                self.needs_redraw = true;
                return false;
            }
            if let Some(action) = self
                .network_hitboxes
                .iter()
                .find(|hitbox| hitbox.y == y && (hitbox.start..hitbox.end).contains(&x))
                .map(|hitbox| hitbox.action)
            {
                self.activate_network_control(action);
                self.needs_redraw = true;
                return false;
            }
            if let Some(action) = self
                .process_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.y == y && (hitbox.start..hitbox.end).contains(&x))
                .map(|hitbox| hitbox.action)
            {
                self.activate_process_control(action);
                self.needs_redraw = true;
                return false;
            }
            if self.process_area.is_some_and(|area| area.contains(x, y)) {
                if let Some(hitbox) = self
                    .process_hitboxes
                    .iter()
                    .copied()
                    .find(|hitbox| hitbox.y == y)
                {
                    if !self.config.pause_processes {
                        self.followed_pid = None;
                    }
                    let clicked_toggle = hitbox
                        .toggle_x
                        .is_some_and(|(start, end)| (start..end).contains(&x));
                    if clicked_toggle {
                        self.selected_process = hitbox.index;
                        self.process_selected = true;
                        self.toggle_process_collapsed(hitbox.pid);
                    } else if self.process_selected && self.selected_process == hitbox.index {
                        self.toggle_process_details();
                    } else {
                        self.selected_process = hitbox.index;
                        self.process_selected = true;
                    }
                } else {
                    self.process_selected = false;
                }
                self.needs_redraw = true;
                return false;
            }
        }
        if button != 0 {
            return false;
        }
        if self.process_selected && !self.process_area.is_some_and(|area| area.contains(x, y)) {
            self.process_selected = false;
            if !self.config.pause_processes {
                self.followed_pid = None;
            }
            self.needs_redraw = true;
        }
        false
    }

    fn select_from_process_scrollbar(&mut self, y: usize) {
        let Some(scrollbar) = self.process_scrollbar else {
            return;
        };
        let track = scrollbar.track_bottom.saturating_sub(scrollbar.track_top);
        if track == 0 || scrollbar.total == 0 {
            return;
        }
        let position = y
            .clamp(
                scrollbar.track_top,
                scrollbar.track_bottom.saturating_sub(1),
            )
            .saturating_sub(scrollbar.track_top);
        self.selected_process = ((position as f64 * (scrollbar.total - 1) as f64
            / track.saturating_sub(1).max(1) as f64)
            .round() as usize)
            .min(scrollbar.total - 1);
        self.process_selected = true;
        if !self.config.pause_processes {
            self.followed_pid = None;
        }
    }
}

pub struct Renderer {
    theme_name: String,
    themes_dir: Option<std::path::PathBuf>,
    palette: theme::Palette,
}

impl Renderer {
    pub fn new() -> Self {
        Self {
            theme_name: String::new(),
            themes_dir: None,
            palette: theme::Palette::default(),
        }
    }

    pub fn render(&mut self, size: Size, app: &mut AppState) -> String {
        let render_started = Instant::now();
        app.draw_times_us = [0; 6];
        app.last_size = Some(size);
        if self.theme_name != app.config.color_theme || self.themes_dir != app.config.themes_dir {
            self.palette =
                theme::Palette::load(&app.config.color_theme, app.config.themes_dir.as_deref())
                    .unwrap_or_default();
            self.theme_name.clone_from(&app.config.color_theme);
            self.themes_dir.clone_from(&app.config.themes_dir);
        }
        let mut canvas = Canvas::new(size.cols as usize, size.rows as usize);
        canvas.palette.clone_from(&self.palette);
        canvas.low_color = app.config.low_color || app.config.tty_mode || !app.config.truecolor;
        canvas.tty = app.config.tty_mode;
        canvas.tty_colors =
            app.config.tty_mode || app.config.color_theme.eq_ignore_ascii_case("TTY");
        canvas.rounded = app.config.rounded_corners && !app.config.tty_mode;
        canvas.graph_symbol = app.config.graph_symbol;
        canvas.theme_background = app.config.theme_background;
        let width = canvas.width;
        let height = canvas.height;
        app.cpu_area = None;
        app.memory_area = None;
        app.network_area = None;
        app.process_area = None;
        let shown_gpus = shown_gpu_panels(&app.config, &app.sample.gpus);
        if !app.config.shown.iter().any(|shown| *shown) && shown_gpus.is_empty() {
            draw_no_boxes(&mut canvas);
        }
        let inline_gpus = inline_gpu_panels(&app.config, &app.sample.gpus, &shown_gpus);
        let lower_visible = app.config.shown[1] || app.config.shown[2] || app.config.shown[3];
        let cpu_h = if app.config.shown[0] {
            if !lower_visible {
                if shown_gpus.is_empty() {
                    height
                } else {
                    height.saturating_sub(
                        shown_gpus
                            .iter()
                            .map(|index| gpu_height_offset(&app.sample.gpus[*index]) + 4)
                            .sum::<usize>(),
                    )
                }
            } else {
                let gpu_divisor = shown_gpus.len() + 1;
                // Draw::calcSizes performs the division in the integer
                // percentage expression before applying ceil(). This differs
                // from treating 32 / divisor as a fraction once three or more
                // dedicated GPU panels are visible.
                let percentage = 32 / gpu_divisor + 5 * usize::from(!shown_gpus.is_empty());
                (height * percentage)
                    .div_ceil(100)
                    .saturating_add(inline_gpus.len())
                    .max(8)
                    .min(height)
            }
        } else {
            0
        };
        let cpu_bottom = app.config.bool_value("cpu_bottom").unwrap_or(false);
        let cpu_y = if cpu_bottom {
            height.saturating_sub(cpu_h)
        } else {
            0
        };
        if app.config.shown[0] {
            canvas.graph_symbol = panel_graph_symbol(&app.config, "graph_symbol_cpu");
            let area = Rect::new(0, cpu_y, width, cpu_h);
            app.cpu_area = Some(area);
            let started = Instant::now();
            draw_cpu(&mut canvas, area, app);
            app.draw_times_us[0] = elapsed_us(started);
        }

        let mut gpu_y = if cpu_bottom { 0 } else { cpu_h };
        let vertical_end = if cpu_bottom { cpu_y } else { height };
        for (position, &index) in shown_gpus.iter().enumerate() {
            let remaining = vertical_end.saturating_sub(gpu_y);
            let gpu_h = gpu_panel_height(
                &app.sample.gpus[index],
                cpu_h,
                lower_visible,
                remaining,
                shown_gpus.len() - position,
                height,
                shown_gpus.len(),
            );
            if gpu_y + gpu_h > vertical_end {
                break;
            }
            canvas.graph_symbol = panel_graph_symbol(&app.config, "graph_symbol_gpu");
            let started = Instant::now();
            draw_gpu(&mut canvas, Rect::new(0, gpu_y, width, gpu_h), app, index);
            app.draw_times_us[4] = app.draw_times_us[4].saturating_add(elapsed_us(started));
            gpu_y += gpu_h;
        }
        let lower_y = gpu_y;
        let gpu_total_height = lower_y.saturating_sub(if cpu_bottom { 0 } else { cpu_h });
        let lower_h = vertical_end.saturating_sub(lower_y);
        let left_visible = app.config.shown[1] || app.config.shown[2];
        let proc_visible = app.config.shown[3];
        let left_w = if left_visible && proc_visible {
            ((width * 45 + 50) / 100)
                .max(36)
                .min(width.saturating_sub(44))
        } else if left_visible {
            width
        } else {
            0
        };
        let proc_left = app.config.bool_value("proc_left").unwrap_or(false);
        let proc_w = width.saturating_sub(left_w);
        let left_x = if proc_left && proc_visible { proc_w } else { 0 };
        let proc_x = if proc_left { 0 } else { left_w };
        if left_visible {
            let mem_h = memory_panel_height(
                height,
                cpu_h,
                gpu_total_height,
                app.config.shown[0],
                app.config.shown[1],
                app.config.shown[2],
                !shown_gpus.is_empty(),
            )
            .min(lower_h);
            let net_h = lower_h.saturating_sub(mem_h);
            let mem_below_net = app.config.bool_value("mem_below_net").unwrap_or(false);
            let mem_y = if mem_below_net && app.config.shown[2] {
                lower_y + net_h
            } else {
                lower_y
            };
            let net_y = if mem_below_net && app.config.shown[1] {
                lower_y
            } else {
                lower_y + mem_h
            };
            if app.config.shown[1] {
                canvas.graph_symbol = panel_graph_symbol(&app.config, "graph_symbol_mem");
                let area = Rect::new(left_x, mem_y, left_w, mem_h);
                app.memory_area = Some(area);
                let started = Instant::now();
                draw_memory(&mut canvas, area, app);
                app.draw_times_us[1] = elapsed_us(started);
            }
            if app.config.shown[2] {
                canvas.graph_symbol = panel_graph_symbol(&app.config, "graph_symbol_net");
                let area = Rect::new(left_x, net_y, left_w, net_h);
                app.network_area = Some(area);
                let started = Instant::now();
                draw_network(&mut canvas, area, app);
                app.draw_times_us[2] = elapsed_us(started);
            }
        }
        if proc_visible {
            canvas.graph_symbol = panel_graph_symbol(&app.config, "graph_symbol_proc");
            let area = Rect::new(proc_x, lower_y, proc_w, lower_h);
            app.process_area = Some(area);
            let started = Instant::now();
            draw_processes(&mut canvas, area, app);
            app.draw_times_us[3] = elapsed_us(started);
        }
        app.draw_times_us[5] = elapsed_us(render_started);
        if app.debug && app.overlay == Overlay::None {
            draw_debug_times(&mut canvas, app);
        }
        if app.overlay != Overlay::None {
            canvas.dim();
        }
        match app.overlay {
            Overlay::Main { selected } => draw_main_menu(&mut canvas, app, selected),
            Overlay::Help => draw_help(&mut canvas, app.help_page),
            Overlay::Options => draw_options(
                &mut canvas,
                app,
                app.options_category,
                app.options_page,
                app.options_selected,
            ),
            Overlay::Signal { pid, signal } => draw_signal(&mut canvas, app, pid, signal),
            Overlay::SignalChoose { pid, selected } => {
                draw_signal_chooser(&mut canvas, app, pid, selected)
            }
            Overlay::Renice { pid, value } => draw_renice(&mut canvas, app, pid, value),
            Overlay::OperationError { operation, errno } => {
                draw_operation_error(&mut canvas, operation, errno)
            }
            Overlay::None => {}
        }
        canvas.finish()
    }
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn draw_debug_times(canvas: &mut Canvas, app: &AppState) {
    if canvas.width < 36 || canvas.height < 11 {
        return;
    }
    let area = Rect::new(1, 1, 33, 9);
    canvas.panel(area, "", theme::PROC_BOX, None);
    canvas.text_bold(
        area.x + 2,
        area.y,
        "box       collect         draw",
        theme::TITLE,
    );
    for (row, (name, index)) in [
        ("cpu", 0),
        ("mem", 1),
        ("net", 2),
        ("proc", 3),
        ("gpu", 4),
        ("total", 5),
    ]
    .into_iter()
    .enumerate()
    {
        let text = format!(
            "{name:<5} {:>12} {:>12}",
            app.sample.collection_times_us[index], app.draw_times_us[index]
        );
        if name == "total" {
            canvas.text_bold(area.x + 1, area.y + row + 2, &text, theme::MAIN);
        } else {
            canvas.text(area.x + 1, area.y + row + 2, &text, theme::MAIN);
        }
    }
}

fn panel_graph_symbol(config: &Config, option: &str) -> GraphSymbol {
    if config.tty_mode {
        return GraphSymbol::Tty;
    }
    match config.value(option).unwrap_or("default") {
        "braille" => GraphSymbol::Braille,
        "block" => GraphSymbol::Block,
        "tty" => GraphSymbol::Tty,
        _ => config.graph_symbol,
    }
}

pub fn minimum_size(config: &Config, gpus: &[GpuSample]) -> Size {
    let cpu = config.shown[0];
    let memory = config.shown[1];
    let network = config.shown[2];
    let processes = config.shown[3];
    let shown_gpus = shown_gpu_panels(config, gpus);

    let mut width = if memory || network { 36 } else { 0 };
    if processes {
        width += 44;
    }
    if cpu {
        width = width.max(60);
    }
    if !shown_gpus.is_empty() {
        width = width.max(41);
    }

    let mut height = if cpu { 8 } else { 0 };
    height += if processes {
        16
    } else {
        usize::from(memory) * 10 + usize::from(network) * 6
    };
    height += shown_gpus
        .iter()
        .map(|index| gpu_height_offset(&gpus[*index]) + 4)
        .sum::<usize>();

    Size {
        cols: width as u16,
        rows: height as u16,
    }
}

pub fn too_small(size: Size, needed: Size) -> String {
    let mut canvas = Canvas::new(size.cols as usize, size.rows as usize);
    let lines = [
        "Terminal size too small:",
        &format!("Width = {} Height = {}", size.cols, size.rows),
        "Needed for current config:",
        &format!("Width = {} Height = {}", needed.cols, needed.rows),
        "Press q to quit",
    ];
    for (index, line) in lines.iter().enumerate() {
        let x = canvas.width.saturating_sub(units::display_width(line)) / 2;
        let y = canvas.height.saturating_sub(lines.len()) / 2 + index;
        canvas.text(
            x,
            y,
            line,
            if index == 0 {
                theme::TITLE
            } else {
                theme::MAIN
            },
        );
    }
    canvas.finish()
}

fn draw_cpu(canvas: &mut Canvas, area: Rect, app: &mut AppState) {
    app.cpu_control_hitboxes.clear();
    if area.h < 3 || area.w < 3 {
        return;
    }
    let cpu = &app.sample.cpu;
    let cpu_temperature_max = cpu.temperature_max.max(1.0);
    let dedicated_gpus = shown_gpu_panels(&app.config, &app.sample.gpus);
    let inline_gpus = inline_gpu_panels(&app.config, &app.sample.gpus, &dedicated_gpus);
    let show_temps = app.config.check_temperature && cpu.temperature.is_some();
    let core_count = cpu.cores.len().max(1);
    let available_rows = area.h.saturating_sub(5 + inline_gpus.len()).max(1);
    let mut columns = core_count.saturating_add(1).div_ceil(available_rows).max(2);
    let right_space = area.w.saturating_sub(area.w / 3);
    let column_size;
    if columns * (21 + 12 * usize::from(show_temps)) < right_space {
        column_size = 2;
    } else if columns * (15 + 6 * usize::from(show_temps)) < right_space {
        column_size = 1;
    } else if columns * (8 + 6 * usize::from(show_temps)) < right_space {
        column_size = 0;
    } else {
        columns = (right_space / (8 + 6 * usize::from(show_temps))).max(1);
        column_size = 0;
    }
    let box_width = if column_size == 0 {
        (8 + 6 * usize::from(show_temps)) * columns + 1
    } else if column_size == 1 {
        (15 + 6 * usize::from(show_temps)) * columns - columns.saturating_sub(1)
    } else {
        (21 + 12 * usize::from(show_temps)) * columns - columns.saturating_sub(1)
    }
    .min(area.w.saturating_sub(4));
    let box_height =
        (core_count.div_ceil(columns) + 4 + inline_gpus.len()).min(area.h.saturating_sub(2));
    let box_x = area.x + area.w - box_width - 1;
    let box_y = area.y + (area.h.saturating_sub(2)).div_ceil(2) - box_height.div_ceil(2) + 1;

    let cpu_bottom = app.config.bool_value("cpu_bottom").unwrap_or(false);
    let controls_y = if cpu_bottom {
        area.y + area.h - 1
    } else {
        area.y
    };
    canvas.panel(
        area,
        if cpu_bottom { "" } else { "¹cpu" },
        theme::CPU_BOX,
        None,
    );
    if cpu_bottom {
        canvas.footer(area.x + 2, controls_y, "¹cpu", theme::CPU_BOX);
    }
    let menu_x = area.x + 10;
    if cpu_bottom {
        canvas.control_footer(menu_x, controls_y, "menu", &[0], true, theme::CPU_BOX);
    } else {
        canvas.control_title(menu_x, controls_y, "menu", &[0], true, theme::CPU_BOX);
    }
    app.cpu_control_hitboxes.push(CpuControlHitbox {
        y: controls_y,
        start: menu_x + 1,
        end: menu_x + 5,
        action: CpuControlAction::Menu,
    });
    let preset = app
        .config
        .preset
        .map(|value| value.to_string())
        .unwrap_or_else(|| "*".into());
    let preset_x = area.x + 16;
    let preset = format!("preset {preset}");
    if cpu_bottom {
        canvas.control_footer(preset_x, controls_y, &preset, &[0], true, theme::CPU_BOX);
    } else {
        canvas.control_title(preset_x, controls_y, &preset, &[0], true, theme::CPU_BOX);
    }
    app.cpu_control_hitboxes.push(CpuControlHitbox {
        y: controls_y,
        start: preset_x + 1,
        end: preset_x + 1 + units::display_width(&preset),
        action: CpuControlAction::Preset,
    });
    if !app.config.clock_format.is_empty() {
        let clock = units::local_clock_format(&app.config.clock_format);
        let clock_x = area.x + area.w / 2 - units::display_width(&clock) / 2;
        if cpu_bottom {
            canvas.footer(clock_x, controls_y, &clock, theme::CPU_BOX);
        } else {
            canvas.title(clock_x, controls_y, &clock, theme::CPU_BOX);
        }
    }
    let update = format!("- {}ms +", app.config.update_ms);
    let update_x = area.x + area.w.saturating_sub(update.len() + 4);
    let update_enabled = !(app.config.process_tree
        && (app.process_selected
            || app
                .followed_pid
                .is_some_and(|pid| app.detailed_pid == Some(pid))));
    if cpu_bottom {
        canvas.control_footer_state(
            update_x,
            controls_y,
            &update,
            &[0, update.chars().count() - 1],
            true,
            update_enabled,
            theme::CPU_BOX,
        );
    } else {
        canvas.control_title_state(
            update_x,
            controls_y,
            &update,
            &[0, update.chars().count() - 1],
            true,
            update_enabled,
            theme::CPU_BOX,
        );
    }
    app.cpu_control_hitboxes.push(CpuControlHitbox {
        y: controls_y,
        start: update_x + 1,
        end: update_x + 3,
        action: CpuControlAction::DecreaseUpdate,
    });
    app.cpu_control_hitboxes.push(CpuControlHitbox {
        y: controls_y,
        start: update_x + update.chars().count() - 1,
        end: update_x + update.chars().count() + 1,
        action: CpuControlAction::IncreaseUpdate,
    });
    if let Some(engine) = &app.sample.cpu.container_engine {
        let engine_x = area.x + 28;
        if engine_x + units::display_width(engine) + 2 < update_x {
            if cpu_bottom {
                canvas.footer_normal(engine_x, controls_y, engine, theme::CPU_BOX);
            } else {
                canvas.title_normal(engine_x, controls_y, engine, theme::CPU_BOX);
            }
        }
    }

    let graph_width = box_x.saturating_sub(area.x + 1);
    let graph_height = area.h.saturating_sub(2);
    let upper_field = app.config.value("cpu_graph_upper").unwrap_or("Auto");
    let lower_field = app.config.value("cpu_graph_lower").unwrap_or("Auto");
    let middle_line = !app.config.cpu_single_graph && upper_field != lower_field;
    let upper_height = if app.config.cpu_single_graph {
        graph_height
    } else {
        graph_height
            .div_ceil(2)
            .saturating_sub(usize::from(middle_line && !graph_height.is_multiple_of(2)))
    };
    let upper_history =
        cpu_graph_history(app, upper_field, &inline_gpus).unwrap_or(&app.cpu_history);
    let lower_history = if lower_field == "Auto" {
        inline_gpus
            .first()
            .and_then(|index| app.gpu_histories.get(*index))
            .map(|history| &history.utilization)
            .unwrap_or(upper_history)
    } else {
        cpu_graph_history(app, lower_field, &inline_gpus).unwrap_or(upper_history)
    };
    draw_graph_options(
        canvas,
        Rect::new(area.x + 1, area.y + 1, graph_width, upper_height),
        upper_history,
        100.0,
        theme::CPU,
        false,
        true,
    );
    if !app.config.cpu_single_graph {
        let lower_height = graph_height.saturating_sub(upper_height + usize::from(middle_line));
        draw_graph_options(
            canvas,
            Rect::new(
                area.x + 1,
                area.y + 1 + upper_height + usize::from(middle_line),
                graph_width,
                lower_height,
            ),
            lower_history,
            100.0,
            theme::CPU,
            app.config.cpu_invert_lower,
            true,
        );
        if middle_line {
            let y = area.y + 1 + upper_height;
            canvas.put(area.x, y, '├', theme::CPU_BOX);
            for x in area.x + 1..box_x {
                canvas.put(x, y, '─', theme::BOX);
            }
            canvas.put(box_x, y, '┤', theme::BOX);
            let label = format!("{upper_field} ▲▼ {lower_field}");
            let x = area.x + graph_width.saturating_sub(units::display_width(&label)) / 2;
            canvas.text(x, y, &label, theme::MAIN);
        }
    }
    if app.config.show_uptime {
        canvas.text(
            area.x + 2,
            area.y + area.h - 2,
            &format!("up {}", units::duration(cpu.uptime)),
            theme::GRAPH_TEXT,
        );
    }

    let frequency = &cpu.frequency;
    let title_room = box_width.saturating_sub(if app.config.show_cpu_frequency { 16 } else { 5 });
    let cpu_name = app
        .config
        .value("custom_cpu_name")
        .filter(|name| !name.is_empty())
        .unwrap_or(&cpu.name);
    canvas.panel(
        Rect::new(box_x, box_y, box_width, box_height),
        &units::truncate(cpu_name, title_room),
        theme::BOX,
        None,
    );
    if app.config.show_cpu_frequency {
        canvas.title(
            box_x + box_width.saturating_sub(frequency.len() + 3),
            box_y,
            frequency,
            theme::BOX,
        );
    }

    let show_watts = app.config.bool_value("show_cpu_watts").unwrap_or(true) && cpu.watts.is_some();
    let show_summary_temp_graph = show_temps && (column_size > 1 || columns > 1);
    let meter_width = box_width
        .saturating_sub(if show_temps {
            23 - 6 * usize::from(!show_summary_temp_graph)
        } else {
            11
        })
        .saturating_sub(if show_watts { 6 } else { 0 });
    canvas.text_bold(box_x + 1, box_y + 1, "CPU ", theme::MAIN);
    meter_bold(
        canvas,
        box_x + 5,
        box_y + 1,
        meter_width,
        cpu.total,
        theme::CPU,
    );
    let percent_x = box_x + 5 + meter_width;
    draw_value_unit_bold(
        canvas,
        percent_x,
        box_y + 1,
        &format!("{:>4.0}", cpu.total),
        "%",
        theme::usage(cpu.total),
        theme::MAIN,
    );
    if let Some(temp) = cpu.temperature.filter(|_| show_temps) {
        let (display_temp, unit) = converted_temperature(temp, &app.config);
        let temperature_x = if show_summary_temp_graph {
            canvas.text(percent_x + 5, box_y + 1, " ", theme::MAIN);
            draw_graph_background(canvas, Rect::new(percent_x + 6, box_y + 1, 5, 1));
            draw_graph_offset_options(
                canvas,
                Rect::new(percent_x + 6, box_y + 1, 5, 1),
                &app.temp_history,
                cpu_temperature_max,
                theme::Style::Temp(100),
                false,
                false,
                -23.0,
            );
            bold_area(canvas, Rect::new(percent_x + 6, box_y + 1, 5, 1));
            percent_x + 11
        } else {
            percent_x + 5
        };
        draw_value_unit_bold(
            canvas,
            temperature_x,
            box_y + 1,
            &format!("{:>4.0}", display_temp),
            unit,
            theme::Style::Temp((temp * 100.0 / cpu_temperature_max).round() as u8),
            theme::MAIN,
        );
    }
    if let Some(watts) = cpu.watts.filter(|_| show_watts) {
        let watts = watts.clamp(0.0, 999.0);
        let value = if watts < 9.995 {
            format!(" {watts:>4.2}")
        } else if watts < 99.95 {
            format!(" {watts:>4.1}")
        } else {
            format!(" {watts:>4.0}")
        };
        let intensity = if app.cpu_watts_max > 0.0 {
            (watts / app.cpu_watts_max * 100.0)
                .round()
                .clamp(0.0, 100.0) as u8
        } else {
            0
        };
        draw_value_unit_bold(
            canvas,
            box_x + box_width.saturating_sub(7),
            box_y + 1,
            &value,
            "W",
            theme::Style::Cached(intensity),
            theme::MAIN,
        );
    }

    let rows_per_column = box_height.saturating_sub(4 + inline_gpus.len()).max(1);
    let column_width = box_width / columns;
    let core_number_width = if core_count >= 100 {
        if column_size == 0 { 3 } else { 4 }
    } else if column_size == 0 {
        2
    } else {
        3
    };
    for (index, value) in cpu.cores.iter().enumerate() {
        let column = index / rows_per_column;
        let row = index % rows_per_column;
        if column >= columns || box_y + row + 2 >= box_y + box_height - 1 {
            continue;
        }
        let x = box_x + column * column_width + 1;
        let y = box_y + row + 2;
        let enabled = cpu
            .active_cpus
            .as_ref()
            .is_none_or(|active| active.contains(&index));
        let core_style = if enabled { theme::MAIN } else { theme::LOW };
        if core_count < 100 {
            canvas.put_bold(x, y, 'C', core_style);
            canvas.text(
                x + 1,
                y,
                &format!("{index:<core_number_width$}"),
                core_style,
            );
        } else {
            canvas.text(x, y, &format!("{index:<core_number_width$}"), core_style);
        }
        let mut cursor = x + core_number_width + usize::from(core_count < 100);
        let core_graph_width = 5 * column_size;
        if core_graph_width > 0 {
            draw_graph_background(canvas, Rect::new(cursor, y, core_graph_width, 1));
            draw_graph_options(
                canvas,
                Rect::new(cursor, y, core_graph_width, 1),
                &app.core_histories[index],
                100.0,
                theme::CPU,
                false,
                false,
            );
            cursor += core_graph_width;
        }
        let percent_width = if column_size < 2 { 3 } else { 4 };
        draw_value_unit(
            canvas,
            cursor,
            y,
            &format!("{:>percent_width$.0}", value),
            "%",
            if enabled {
                theme::usage(*value)
            } else {
                theme::LOW
            },
            core_style,
        );
        cursor += percent_width + 1;
        if show_temps
            && app.config.show_core_temperature
            && let Some(temp) = cpu.core_temperatures.get(index).copied().flatten()
        {
            let (display_temp, unit) = converted_temperature(temp, &app.config);
            if column_size > 1 {
                cursor += 1;
                draw_graph_background(canvas, Rect::new(cursor, y, 5, 1));
                draw_graph_offset_options(
                    canvas,
                    Rect::new(cursor, y, 5, 1),
                    &app.core_temperature_histories[index],
                    cpu_temperature_max,
                    theme::Style::Temp(100),
                    false,
                    false,
                    -23.0,
                );
                cursor += 5;
            }
            draw_value_unit(
                canvas,
                cursor,
                y,
                &format!("{:>4.0}", display_temp),
                unit,
                if enabled {
                    theme::Style::Temp((temp * 100.0 / cpu_temperature_max).round() as u8)
                } else {
                    theme::LOW
                },
                core_style,
            );
        }
        if column + 1 < columns {
            canvas.put(box_x + (column + 1) * column_width, y, '│', theme::BOX);
        }
    }
    let load = format!(
        "Load avg: {:.2} {:.2} {:.2}",
        cpu.load[0], cpu.load[1], cpu.load[2]
    );
    canvas.text_bold(
        box_x + box_width.saturating_sub(load.len() + 1),
        box_y + box_height - 2 - inline_gpus.len(),
        "Load avg:",
        theme::MAIN,
    );
    canvas.text(
        box_x + box_width.saturating_sub(load.len() + 1) + 9,
        box_y + box_height - 2 - inline_gpus.len(),
        &load[9..],
        theme::MAIN,
    );

    for (row, &index) in inline_gpus.iter().enumerate() {
        let gpu = &app.sample.gpus[index];
        let history = &app.gpu_histories[index];
        let y = box_y + box_height - 1 - inline_gpus.len() + row;
        let prefix = if app.sample.gpus.len() > 1 {
            format!("GPU{index}")
        } else {
            "GPU".into()
        };
        canvas.text_bold(box_x + 1, y, &prefix, theme::MAIN);
        let mut x = box_x + 1 + prefix.len();
        if gpu.support.utilization {
            let meter_width = if columns > 1 { 5 } else { 0 };
            if meter_width > 0 {
                meter_bold(
                    canvas,
                    x + 1,
                    y,
                    meter_width,
                    f64::from(gpu.utilization),
                    theme::CPU,
                );
            }
            x += meter_width + 1;
            draw_value_unit_bold(
                canvas,
                x,
                y,
                &format!("{:>3}", gpu.utilization),
                "%",
                theme::usage(f64::from(gpu.utilization)),
                theme::MAIN,
            );
            x += 4;
        }
        if gpu.support.memory_used {
            if columns > 1 && gpu.support.memory_total {
                draw_graph_background(canvas, Rect::new(x + 1, y, 5, 1));
                draw_graph(
                    canvas,
                    Rect::new(x + 1, y, 5, 1),
                    &history.memory_used,
                    100.0,
                    theme::Style::Used(100),
                );
                bold_area(canvas, Rect::new(x + 1, y, 5, 1));
                x += 6;
            }
            let used = units::bytes_short(gpu.memory_used, app.config.base_10_sizes);
            canvas.text_bold(x, y, &format!("{used:>5}"), theme::MAIN);
            x += 5;
        }
        if gpu.support.memory_total {
            let total = units::bytes_short(gpu.memory_total, app.config.base_10_sizes);
            if gpu.support.memory_used {
                canvas.put_bold(x, y, '/', theme::LOW);
                canvas.text_bold(x + 1, y, &format!("{total:<4}"), theme::MAIN);
            } else {
                canvas.text_bold(x, y, &format!("{total:<5}"), theme::MAIN);
            }
            x += 5;
        }
        if show_temps && gpu.support.temperature {
            let (display_temp, unit) = converted_temperature(gpu.temperature_c as f64, &app.config);
            if columns > 1 {
                draw_graph_background(canvas, Rect::new(x + 1, y, 5, 1));
                draw_graph_offset_options(
                    canvas,
                    Rect::new(x + 1, y, 5, 1),
                    &history.temperature,
                    gpu.temperature_max_c.max(1) as f64,
                    theme::Style::Temp(100),
                    false,
                    false,
                    -23.0,
                );
                bold_area(canvas, Rect::new(x + 1, y, 5, 1));
                x += 6;
            }
            draw_value_unit_bold(
                canvas,
                x,
                y,
                &format!("{:>3.0}", display_temp),
                unit,
                theme::Style::Temp(
                    (gpu.temperature_c * 100 / gpu.temperature_max_c.max(1)).clamp(0, 100) as u8,
                ),
                theme::MAIN,
            );
            x += 5;
        }
        if gpu.support.power {
            let watts = gpu.power_mw as f64 / 1000.0;
            let power = if gpu.power_mw < 10_000 {
                format!("{watts:>4.2}")
            } else if gpu.power_mw < 100_000 {
                format!("{watts:>4.1}")
            } else {
                format!("{watts:>4.0}")
            };
            draw_value_unit_bold(
                canvas,
                x + 1,
                y,
                &power,
                "W",
                theme::Style::Cached(
                    ratio(gpu.power_mw, gpu.power_limit_mw)
                        .round()
                        .clamp(0.0, 100.0) as u8,
                ),
                theme::MAIN,
            );
        }
    }

    if app.config.bool_value("show_battery").unwrap_or(true)
        && let Some(battery) = &cpu.battery
    {
        let symbol = match battery.status.to_ascii_lowercase().as_str() {
            "charging" => "▲",
            "discharging" => "▼",
            "full" => "■",
            _ => "○",
        };
        let watts = if app.config.bool_value("show_battery_watts").unwrap_or(true) {
            battery
                .watts
                .map(|value| format!("{value:.2}W"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        let time = battery
            .seconds
            .filter(|seconds| *seconds > 0)
            .map(battery_duration)
            .unwrap_or_default();
        let prefix = format!("BAT{symbol} {}%", battery.percent);
        let source_length = usize::from(canvas.width >= 100) * 11
            + units::display_width(&battery.percent.to_string())
            + 1
            + units::display_width(&time)
            + units::display_width(&watts)
            + app.config.update_ms.to_string().len();
        let x = canvas.width.saturating_sub(source_length + 18);
        canvas.put(
            x,
            controls_y,
            if cpu_bottom { '┘' } else { '┐' },
            theme::CPU_BOX,
        );
        let mut cursor = x + 1;
        canvas.text_bold(cursor, controls_y, &prefix, theme::TITLE);
        cursor += units::display_width(&prefix);
        if canvas.width >= 100 {
            canvas.put(cursor, controls_y, ' ', theme::TITLE);
            meter_inverted(
                canvas,
                cursor + 1,
                controls_y,
                10,
                f64::from(battery.percent),
                theme::CPU,
            );
            cursor += 11;
        }
        for value in [&time, &watts] {
            if !value.is_empty() {
                canvas.put_bold(cursor, controls_y, ' ', theme::TITLE);
                cursor += 1;
                canvas.text_bold(cursor, controls_y, value, theme::TITLE);
                cursor += units::display_width(value);
            }
        }
        canvas.put(
            cursor,
            controls_y,
            if cpu_bottom { '└' } else { '┌' },
            theme::CPU_BOX,
        );
    }
}

fn battery_duration(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = seconds % 86_400 / 3_600;
    let minutes = seconds % 3_600 / 60;
    if days > 0 {
        format!("{days}d {hours:02}:{minutes:02}")
    } else {
        format!("{hours:02}:{minutes:02}")
    }
}

fn converted_temperature(celsius: f64, config: &Config) -> (f64, &'static str) {
    match config.value("temp_scale").unwrap_or("celsius") {
        "fahrenheit" => (celsius * 9.0 / 5.0 + 32.0, "°F"),
        "kelvin" => (celsius + 273.15, "K"),
        "rankine" => ((celsius + 273.15) * 9.0 / 5.0, "°R"),
        _ => (celsius, "°C"),
    }
}

fn cpu_graph_history<'a>(
    app: &'a AppState,
    field: &str,
    inline_gpus: &[usize],
) -> Option<&'a VecDeque<f64>> {
    match field {
        "Auto" | "total" => Some(&app.cpu_history),
        "gpu-totals" => inline_gpus
            .first()
            .and_then(|index| app.gpu_histories.get(*index))
            .map(|history| &history.utilization),
        "gpu-vram-totals" => inline_gpus
            .first()
            .and_then(|index| app.gpu_histories.get(*index))
            .map(|history| &history.memory_used),
        "gpu-pwr-totals" => inline_gpus
            .first()
            .and_then(|index| app.gpu_histories.get(*index))
            .map(|history| &history.power),
        name => app.cpu_field_histories.get(name),
    }
}

fn shown_gpu_panels(config: &Config, gpus: &[GpuSample]) -> Vec<usize> {
    let Some(boxes) = config.value("shown_boxes") else {
        return Vec::new();
    };
    boxes
        .split_whitespace()
        .filter_map(|name| name.strip_prefix("gpu")?.parse::<usize>().ok())
        .filter(|index| *index < gpus.len())
        .collect()
}

fn inline_gpu_panels(config: &Config, gpus: &[GpuSample], dedicated: &[usize]) -> Vec<usize> {
    match config.value("show_gpu_info").unwrap_or("Auto") {
        "On" => (0..gpus.len()).collect(),
        "Auto" => (0..gpus.len())
            .filter(|index| !dedicated.contains(index))
            .collect(),
        _ => Vec::new(),
    }
}

fn gpu_height_offset(gpu: &GpuSample) -> usize {
    usize::from(gpu.support.utilization)
        + usize::from(gpu.support.power)
        + usize::from(gpu.support.encoder || gpu.support.decoder)
        + usize::from(gpu.support.memory_total || gpu.support.memory_used)
            * (1 + 2 * usize::from(gpu.support.memory_total && gpu.support.memory_used)
                + 2 * usize::from(gpu.support.memory_utilization))
}

fn gpu_panel_height(
    gpu: &GpuSample,
    cpu_height: usize,
    lower_visible: bool,
    remaining_height: usize,
    remaining_gpus: usize,
    terminal_height: usize,
    total_gpus: usize,
) -> usize {
    let mut base = if cpu_height > 0 && lower_visible {
        cpu_height
    } else if cpu_height == 0 && !lower_visible {
        remaining_height.div_ceil(remaining_gpus.max(1))
    } else if cpu_height == 0 {
        (terminal_height * 32)
            .div_ceil(100 * total_gpus.max(1))
            .max(8)
    } else {
        8
    };
    if cpu_height > 0 && base + cpu_height == terminal_height.saturating_sub(1) {
        base += 1;
    }
    base.max(gpu_height_offset(gpu) + 4)
}

fn memory_panel_height(
    terminal_height: usize,
    cpu_height: usize,
    gpu_height: usize,
    cpu_shown: bool,
    memory_shown: bool,
    network_shown: bool,
    gpu_shown: bool,
) -> usize {
    if !memory_shown {
        return 0;
    }
    let lower_height = terminal_height.saturating_sub(cpu_height + gpu_height);
    if !network_shown {
        return lower_height;
    }

    // btop calculates memory from the full terminal height. With a dedicated
    // GPU below the CPU it compresses Net::height_p (28) by 4/5 first.
    let divisor = usize::from(gpu_shown && cpu_shown) + 4;
    let network_percent = 28 * 4 / divisor;
    (terminal_height * (100 - network_percent) / 100)
        .saturating_sub(cpu_height + gpu_height)
        .min(lower_height)
}

fn draw_gpu(canvas: &mut Canvas, area: Rect, app: &AppState, index: usize) {
    if area.h < 4 || area.w < 41 {
        return;
    }
    let gpu = &app.sample.gpus[index];
    let history = &app.gpu_histories[index];
    let key = ["⁵", "⁶", "⁷", "⁸", "⁹", "⁰"]
        .get(index)
        .copied()
        .unwrap_or("");
    canvas.panel(area, &format!("{key}gpu{index}"), theme::CPU_BOX, None);

    let box_width = (area.w / 2).clamp(41, 65);
    let box_height = (gpu_height_offset(gpu) + 2).min(area.h.saturating_sub(2));
    let box_x = area.x + area.w - box_width - 1;
    let box_y = area.y + (area.h.saturating_sub(2 + box_height)).div_ceil(2) + 1;
    let custom_name = app
        .config
        .value(&format!("custom_gpu_name{index}"))
        .filter(|name| !name.is_empty())
        .unwrap_or(&gpu.name);
    canvas.panel(
        Rect::new(box_x, box_y, box_width, box_height),
        &units::truncate(custom_name, box_width.saturating_sub(5)),
        theme::BOX,
        None,
    );

    let graph_width = box_x.saturating_sub(area.x + 1);
    let graph_height = area.h.saturating_sub(2);
    let mirrored = app.config.bool_value("gpu_mirror_graph").unwrap_or(true);
    let upper_height = if mirrored {
        graph_height.div_ceil(2)
    } else {
        graph_height
    };
    if gpu.support.utilization {
        draw_graph_options(
            canvas,
            Rect::new(area.x + 1, area.y + 1, graph_width, upper_height),
            &history.utilization,
            100.0,
            theme::CPU,
            false,
            true,
        );
        if mirrored {
            draw_graph_options(
                canvas,
                Rect::new(
                    area.x + 1,
                    area.y + 1 + upper_height,
                    graph_width,
                    graph_height.saturating_sub(upper_height),
                ),
                &history.utilization,
                100.0,
                theme::CPU,
                app.config.cpu_invert_lower,
                true,
            );
        }
    }

    if gpu.support.gpu_clock {
        let clock = format!("{} MHz", gpu.gpu_clock_mhz);
        canvas.title(
            box_x + box_width.saturating_sub(clock.len() + 3),
            box_y,
            &clock,
            theme::BOX,
        );
    }

    let mut row = box_y + 1;
    if gpu.support.utilization && row < box_y + box_height - 1 {
        let show_temp = app.config.check_temperature && gpu.support.temperature;
        let meter_width = box_width.saturating_sub(if show_temp { 25 } else { 12 });
        canvas.text_bold(box_x + 1, row, "GPU ", theme::MAIN);
        meter_bold(
            canvas,
            box_x + 5,
            row,
            meter_width,
            f64::from(gpu.utilization),
            theme::CPU,
        );
        let value_x = box_x + 5 + meter_width;
        draw_value_unit_bold(
            canvas,
            value_x,
            row,
            &format!("{:>5}", gpu.utilization),
            "%",
            theme::usage(f64::from(gpu.utilization)),
            theme::MAIN,
        );
        if show_temp {
            let (display_temp, unit) = converted_temperature(gpu.temperature_c as f64, &app.config);
            let max = gpu.temperature_max_c.max(24) as f64;
            draw_graph_background(canvas, Rect::new(value_x + 7, row, 6, 1));
            draw_graph_offset_options(
                canvas,
                Rect::new(value_x + 7, row, 6, 1),
                &history.temperature,
                max,
                theme::Style::Temp(100),
                false,
                false,
                -23.0,
            );
            bold_area(canvas, Rect::new(value_x + 7, row, 6, 1));
            draw_value_unit_bold(
                canvas,
                value_x + 13,
                row,
                &format!("{:>4.0}", display_temp),
                unit,
                theme::Style::Temp(
                    (gpu.temperature_c * 100 / gpu.temperature_max_c.max(1)).clamp(0, 100) as u8,
                ),
                theme::MAIN,
            );
        }
        row += 1;
    }

    if gpu.support.power && row < box_y + box_height - 1 {
        let show_state = gpu.support.power_state && gpu.power_state != 32;
        let meter_width = box_width.saturating_sub(if show_state { 25 } else { 12 });
        let power_percent = ratio(gpu.power_mw, gpu.power_limit_mw);
        canvas.text_bold(box_x + 1, row, "PWR ", theme::MAIN);
        meter_bold(
            canvas,
            box_x + 5,
            row,
            meter_width,
            power_percent,
            theme::Style::Cached(100),
        );
        let watts = gpu.power_mw as f64 / 1000.0;
        let value = if gpu.power_mw < 10_000 {
            format!("{watts:>5.2}")
        } else if gpu.power_mw < 100_000 {
            format!("{watts:>5.1}")
        } else {
            format!("{watts:>5.0}")
        };
        draw_value_unit_bold(
            canvas,
            box_x + 5 + meter_width,
            row,
            &value,
            "W",
            theme::Style::Cached(power_percent.round().clamp(0.0, 100.0) as u8),
            theme::MAIN,
        );
        if show_state {
            let state_x = box_x + 5 + meter_width + units::display_width(&value) + 1;
            canvas.text_bold(
                state_x,
                row,
                &format!(" P-state: {}P", if gpu.power_state <= 9 { " " } else { "" }),
                theme::MAIN,
            );
            let state_prefix_width = 11 + usize::from(gpu.power_state <= 9);
            canvas.text_bold(
                state_x + state_prefix_width,
                row,
                &gpu.power_state.to_string(),
                theme::Style::Cached(gpu.power_state.clamp(0, 100) as u8),
            );
        }
        row += 1;
    }

    if gpu.support.encoder && gpu.support.decoder && row < box_y + box_height - 1 {
        let half = box_width.saturating_sub(20) / 2;
        canvas.text_bold(box_x + 1, row, "ENC ", theme::MAIN);
        meter_bold(
            canvas,
            box_x + 5,
            row,
            half,
            f64::from(gpu.encoder_utilization),
            theme::CPU,
        );
        let encoder_value_x = box_x + 5 + half;
        draw_value_unit_bold(
            canvas,
            encoder_value_x,
            row,
            &format!("{:>4}", gpu.encoder_utilization),
            "%",
            theme::usage(f64::from(gpu.encoder_utilization)),
            theme::MAIN,
        );
        canvas.put_bold(encoder_value_x + 5, row, '│', theme::BOX);
        canvas.text_bold(encoder_value_x + 6, row, "DEC ", theme::MAIN);
        let decoder_x = box_x + 15 + half;
        let decoder_value = format!("{:>4}", gpu.decoder_utilization);
        let stats_right = box_x + box_width - 1;
        let decoder_value_x = stats_right.saturating_sub(units::display_width(&decoder_value) + 1);
        meter_bold(
            canvas,
            decoder_x,
            row,
            decoder_value_x.saturating_sub(decoder_x),
            f64::from(gpu.decoder_utilization),
            theme::CPU,
        );
        draw_value_unit_bold(
            canvas,
            decoder_value_x,
            row,
            &decoder_value,
            "%",
            theme::usage(f64::from(gpu.decoder_utilization)),
            theme::MAIN,
        );
        row += 1;
    }

    if gpu.support.memory_total && gpu.support.memory_used && row + 4 < box_y + box_height {
        let percentage = ratio(gpu.memory_used, gpu.memory_total);
        let middle = box_x + box_width / 2;
        let right = box_x + box_width - 1;
        for x in box_x + 1..right {
            canvas.put(x, row, '─', theme::BOX);
        }
        canvas.put(box_x, row, '├', theme::BOX);
        canvas.put(middle, row, '┬', theme::BOX);
        canvas.put(right, row, '┤', theme::BOX);
        canvas.title(box_x + 2, row, "vram", theme::BOX);
        if gpu.support.memory_clock {
            let clock = format!("{} MHz", gpu.memory_clock_mhz);
            canvas.title(
                middle.saturating_sub(clock.len() + 2),
                row,
                &clock,
                theme::BOX,
            );
        }
        let used = units::bytes(gpu.memory_used, app.config.base_10_sizes);
        canvas.text(middle + 2, row, "Used:", theme::TITLE);
        canvas.text(
            right.saturating_sub(used.len() + 1),
            row,
            &used,
            theme::TITLE,
        );
        for y in row + 1..=row + 4 {
            canvas.put(box_x, y, '│', theme::BOX);
            canvas.put(middle, y, '│', theme::BOX);
            canvas.put(right, y, '│', theme::BOX);
        }
        let total = units::bytes(gpu.memory_total, app.config.base_10_sizes);
        canvas.text_bold(box_x + 2, row + 1, "Total:", theme::MAIN);
        canvas.text_bold(
            middle.saturating_sub(total.len() + 1),
            row + 1,
            &total,
            theme::MAIN,
        );
        draw_graph(
            canvas,
            Rect::new(
                middle + 1,
                row + 1,
                box_width / 2 - 2,
                2 + 2 * usize::from(gpu.support.memory_utilization),
            ),
            &history.memory_used,
            100.0,
            theme::Style::Used(100),
        );
        canvas.text(
            middle + 2,
            row + 1,
            &format!("{:>3.0}%", percentage),
            theme::MAIN,
        );
        if gpu.support.memory_utilization {
            for x in box_x + 1..middle {
                canvas.put(x, row + 2, '─', theme::BOX);
            }
            canvas.put(box_x, row + 2, '├', theme::BOX);
            canvas.put(middle, row + 2, '┤', theme::BOX);
            canvas.text(box_x + 2, row + 2, "Utilization:", theme::TITLE);
            draw_graph_offset(
                canvas,
                Rect::new(box_x + 1, row + 3, box_width / 2 - 1, 2),
                &history.memory_utilization,
                100.0,
                theme::Style::Free(100),
                4.0,
            );
            canvas.text(
                box_x + 1,
                row + 3,
                &format!("{:>3}%", gpu.memory_utilization),
                theme::MAIN,
            );
        }
    } else if (gpu.support.memory_total || gpu.support.memory_used) && row < box_y + box_height - 1
    {
        let value = if gpu.support.memory_total {
            gpu.memory_total
        } else {
            gpu.memory_used
        };
        let label = if gpu.support.memory_total {
            "VRAM total:"
        } else {
            "VRAM usage:"
        };
        canvas.text(
            box_x + 1,
            row,
            &format!("{label} {}", units::bytes(value, app.config.base_10_sizes)),
            theme::MAIN,
        );
    }

    if gpu.support.pcie && gpu.pcie_tx_kib >= 0 && gpu.pcie_rx_kib >= 0 {
        let bottom = box_y + box_height - 1;
        let middle = box_x + box_width / 2;
        let tx = format!(
            "{}/s",
            units::bytes(gpu.pcie_tx_kib as u64 * 1024, app.config.base_10_sizes)
        );
        let rx = format!(
            "{}/s",
            units::bytes(gpu.pcie_rx_kib as u64 * 1024, app.config.base_10_sizes)
        );
        canvas.footer(box_x + 2, bottom, "TX:", theme::BOX);
        canvas.footer(middle.saturating_sub(tx.len() + 2), bottom, &tx, theme::BOX);
        canvas.put(middle, bottom, '┴', theme::BOX);
        canvas.footer(middle + 1, bottom, "RX:", theme::BOX);
        canvas.footer(
            box_x + box_width.saturating_sub(rx.len() + 2),
            bottom,
            &rx,
            theme::BOX,
        );
    }
}

fn draw_inline_swap_header(
    canvas: &mut Canvas,
    area: Rect,
    divider: usize,
    mut cy: usize,
    graph_height: usize,
    total: u64,
    base_10: bool,
) -> Option<usize> {
    if cy > area.h.saturating_sub(5) {
        return None;
    }
    if area.h.saturating_sub(cy) > 6 {
        if graph_height > 0 {
            let y = area.y + 1 + cy;
            canvas.put(area.x, y, '├', theme::MEM_BOX);
            for x in area.x + 1..divider {
                canvas.put(x, y, '─', theme::BOX);
            }
            canvas.put(divider, y, '┤', theme::MEM_BOX);
        }
        cy += 1;
    }
    let y = area.y + 1 + cy;
    canvas.text_bold(area.x + 2, y, "Swap:", theme::TITLE);
    let human = units::bytes_spaced(total, base_10);
    canvas.text_preserve_spaces_bold(
        divider.saturating_sub(human.len() + 1),
        y,
        &human,
        theme::TITLE,
    );
    Some(cy + 1)
}

fn draw_memory(canvas: &mut Canvas, area: Rect, app: &mut AppState) {
    app.memory_control_hitboxes.clear();
    if area.h < 3 || area.w < 3 {
        return;
    }
    let mem = &app.sample.memory;
    let show_disks = app.config.show_disks;
    let mut mem_width = if show_disks {
        (area.w.saturating_sub(3)).div_ceil(2)
    } else {
        area.w.saturating_sub(1)
    };
    if show_disks && mem_width % 2 != 0 {
        mem_width += 1;
    }
    let divider = area.x + mem_width;
    let disks_width = area.w.saturating_sub(mem_width + 2);
    let use_graphs = app.config.mem_graphs;
    let has_inline_swap = mem.swap_total > 0
        && app.config.bool_value("show_swap").unwrap_or(true)
        && !app.config.bool_value("swap_disk").unwrap_or(true);
    let item_height = if has_inline_swap { 6 } else { 4 };
    let mem_size = if area.h.saturating_sub(if has_inline_swap { 3 } else { 2 }) > 2 * item_height {
        3
    } else if mem_width > 25 {
        2
    } else {
        1
    };
    let graph_height = if use_graphs {
        let reserved = if has_inline_swap { 2 } else { 1 };
        let groups = if mem_size == 3 { 2 } else { 1 };
        let available = area.h.saturating_sub(reserved + groups * item_height);
        ((available as f64 / item_height as f64).round() as usize).max(1)
    } else {
        0
    };
    let mut graph_width = mem_width.saturating_sub(if mem_size > 2 { 7 } else { 17 });
    if mem_size == 1 {
        graph_width += 6;
    }
    if graph_height > 1 {
        graph_width += 6;
    }

    canvas.panel(area, "²mem", theme::MEM_BOX, None);
    let disks_x = if show_disks {
        divider + 2
    } else {
        area.x + area.w - 9
    };
    canvas.control_title(disks_x, area.y, "disks", &[0], show_disks, theme::MEM_BOX);
    app.memory_control_hitboxes.push(MemoryControlHitbox {
        y: area.y,
        start: disks_x + 1,
        end: disks_x + 6,
        action: MemoryControlAction::Disks,
    });
    if show_disks {
        let io_x = area.x + area.w - 6;
        canvas.control_title(
            io_x,
            area.y,
            "io",
            &[0],
            app.config.bool_value("io_mode").unwrap_or(false),
            theme::MEM_BOX,
        );
        app.memory_control_hitboxes.push(MemoryControlHitbox {
            y: area.y,
            start: io_x + 1,
            end: io_x + 3,
            action: MemoryControlAction::IoMode,
        });
        canvas.put(divider, area.y, '┬', theme::MEM_BOX);
        canvas.put(divider, area.y + area.h - 1, '┴', theme::MEM_BOX);
        for y in area.y + 1..area.y + area.h - 1 {
            canvas.put(divider, y, '│', theme::BOX);
        }
    }

    let total = units::bytes_spaced(mem.total, app.config.base_10_sizes);
    canvas.text_bold(
        area.x + 2,
        area.y + 1,
        &format!("Total:{total:>width$}", width = mem_width.saturating_sub(9)),
        theme::TITLE,
    );

    let mut entries = vec![
        (
            "Used",
            mem.used,
            mem.total,
            &app.mem_history,
            theme::Style::Used(100),
            false,
        ),
        (
            "Available",
            mem.available,
            mem.total,
            &app.available_history,
            theme::Style::Available(100),
            false,
        ),
        (
            "Cached",
            mem.cached,
            mem.total,
            &app.cached_history,
            theme::Style::Cached(100),
            false,
        ),
        (
            "Free",
            mem.free,
            mem.total,
            &app.free_history,
            theme::Style::Free(100),
            false,
        ),
    ];
    if has_inline_swap {
        entries.push((
            "Used",
            mem.swap_used,
            mem.swap_total,
            &app.swap_used_history,
            theme::Style::Used(100),
            true,
        ));
        entries.push((
            "Free",
            mem.swap_total.saturating_sub(mem.swap_used),
            mem.swap_total,
            &app.swap_free_history,
            theme::Style::Free(100),
            false,
        ));
    }
    let compact = mem_size < 3;
    let mut cy = 1usize;
    if compact {
        for (title, value, basis, history, style, swap_start) in entries {
            if swap_start {
                let Some(next) = draw_inline_swap_header(
                    canvas,
                    area,
                    divider,
                    cy,
                    graph_height,
                    mem.swap_total,
                    app.config.base_10_sizes,
                ) else {
                    break;
                };
                cy = next;
            }
            if cy + 1 >= area.h - 1 {
                break;
            }
            let y = area.y + 1 + cy;
            let short_title: String = title
                .chars()
                .take(if mem_size > 1 { 5 } else { 1 })
                .collect();
            canvas.text(
                area.x + 2,
                y,
                &format!(
                    "{short_title:<width$}",
                    width = if mem_size > 1 { 5 } else { 1 }
                ),
                theme::MAIN,
            );
            let graph_x =
                area.x + 2 + if mem_size > 1 { 5 } else { 1 } + usize::from(graph_height < 2);
            if use_graphs {
                draw_graph(
                    canvas,
                    Rect::new(graph_x, y, graph_width, graph_height),
                    history,
                    100.0,
                    style,
                );
            } else {
                meter(canvas, graph_x, y, graph_width, ratio(value, basis), style);
            }
            let human = units::bytes_spaced(value, app.config.base_10_sizes);
            canvas.text(
                divider.saturating_sub(1 + if mem_size > 1 { 9 } else { 7 }),
                y,
                &format!(
                    "{:>width$}",
                    human,
                    width = if mem_size > 1 { 9 } else { 7 }
                ),
                theme::TITLE,
            );
            cy += graph_height.max(1);
        }
        if cy < area.h - 2 {
            let y = area.y + 1 + cy;
            canvas.put(area.x, y, '├', theme::MEM_BOX);
            for x in area.x + 1..divider {
                canvas.put(x, y, '─', theme::BOX);
            }
            canvas.put(divider, y, '┤', theme::MEM_BOX);
        }
    } else {
        for (title, value, basis, history, style, swap_start) in entries {
            if swap_start {
                let Some(next) = draw_inline_swap_header(
                    canvas,
                    area,
                    divider,
                    cy,
                    graph_height,
                    mem.swap_total,
                    app.config.base_10_sizes,
                ) else {
                    break;
                };
                cy = next;
            }
            if cy + 1 >= area.h - 1 {
                break;
            }
            let y = area.y + 1 + cy;
            canvas.put(area.x, y, '├', theme::MEM_BOX);
            for x in area.x + 1..divider {
                canvas.put(x, y, '─', theme::BOX);
            }
            canvas.put(divider, y, '┤', theme::MEM_BOX);
            let human = units::bytes_spaced(value, app.config.base_10_sizes);
            canvas.text(area.x + 2, y, &format!("{title}:"), theme::MAIN);
            canvas.text_preserve_spaces(
                divider.saturating_sub(human.len() + 1),
                y,
                &human,
                theme::MAIN,
            );
            let draw_height = if use_graphs {
                graph_height.min(area.bottom().saturating_sub(y + 2))
            } else {
                usize::from(y + 1 < area.bottom())
            };
            if draw_height > 0 {
                let graph_x = area.x + 1 + usize::from(graph_height < 2);
                if use_graphs {
                    draw_graph(
                        canvas,
                        Rect::new(graph_x, y + 1, graph_width, draw_height),
                        history,
                        100.0,
                        style,
                    );
                } else {
                    meter(
                        canvas,
                        graph_x,
                        y + 1,
                        graph_width,
                        ratio(value, basis),
                        style,
                    );
                }
                let percent_x = if graph_height >= 2 {
                    area.x + 2
                } else {
                    graph_x + graph_width
                };
                canvas.text(
                    percent_x,
                    y + 1,
                    &format!("{:>3.0}%", ratio(value, basis)),
                    theme::MAIN,
                );
            }
            cy += if use_graphs { graph_height + 1 } else { 2 };
        }
        if use_graphs && cy < area.h - 2 {
            let separator_y = area.y + 1 + cy;
            canvas.put(area.x, separator_y, '├', theme::MEM_BOX);
            for x in area.x + 1..divider {
                canvas.put(x, separator_y, '─', theme::BOX);
            }
            canvas.put(divider, separator_y, '┤', theme::MEM_BOX);
        }
    }
    if show_disks {
        let io_mode = app.config.bool_value("io_mode").unwrap_or(false);
        let show_io_stat = app.config.bool_value("show_io_stat").unwrap_or(true);
        let io_combined = app.config.bool_value("io_graph_combined").unwrap_or(false);
        if io_mode {
            draw_disks_io(canvas, area, divider, disks_width, app, io_combined);
        } else {
            draw_disks_capacity(canvas, area, divider, disks_width, app, show_io_stat);
        }
    }
}

fn draw_disk_divider(canvas: &mut Canvas, area: Rect, divider: usize, y: usize) {
    canvas.put(divider, y, '├', theme::BOX);
    for x in divider + 1..area.x + area.w - 1 {
        canvas.put(x, y, '─', theme::BOX);
    }
    canvas.put(area.x + area.w - 1, y, '┤', theme::MEM_BOX);
}

fn disk_name(mount: &str) -> String {
    if mount == "/" {
        "root".to_string()
    } else {
        mount
            .rsplit('/')
            .find(|part| !part.is_empty())
            .unwrap_or("disk")
            .to_string()
    }
}

fn draw_disks_capacity(
    canvas: &mut Canvas,
    area: Rect,
    divider: usize,
    disks_width: usize,
    app: &AppState,
    show_io_stat: bool,
) {
    let meter_width = disks_width.saturating_sub(21).max(1);
    let mut cy = 0usize;
    let swap = crate::collect::DiskSample {
        mount: "swap".into(),
        total: app.sample.memory.swap_total,
        used: app.sample.memory.swap_used,
        free: app
            .sample
            .memory
            .swap_total
            .saturating_sub(app.sample.memory.swap_used),
        ..crate::collect::DiskSample::default()
    };
    let mut disks: Vec<_> = app.sample.memory.disks.iter().collect();
    if app.sample.memory.swap_total > 0
        && app.config.bool_value("show_swap").unwrap_or(true)
        && app.config.bool_value("swap_disk").unwrap_or(true)
    {
        disks.insert(0, &swap);
    }
    let disk_ios = disks.iter().filter(|disk| disk.io_supported).count();
    let io_rows = usize::from(show_io_stat) * disk_ios;
    let show_free = disks.len() * 3 + io_rows <= area.h.saturating_sub(1);
    let roomy_gap = disks.len() * 4 + io_rows <= area.h.saturating_sub(1);
    for disk in disks {
        let io_row = usize::from(show_io_stat && disk.io_supported);
        let disk_rows = 2 + io_row + usize::from(show_free) + usize::from(roomy_gap);
        if cy + disk_rows > area.h - 1 {
            break;
        }
        let y = area.y + 1 + cy;
        draw_disk_divider(canvas, area, divider, y);
        let name = disk_name(&disk.mount);
        canvas.text_bold(divider + 2, y, &name, theme::TITLE);
        let total = units::bytes_spaced(disk.total, app.config.base_10_sizes);
        canvas.text_preserve_spaces_bold(
            area.x + area.w.saturating_sub(total.len() + 2),
            y,
            &total,
            theme::TITLE,
        );
        let io_label = disk_io_label(
            disk.read_per_second,
            disk.write_per_second,
            app.config.base_10_sizes,
        );
        if disks_width >= 25
            && let Some(label) = &io_label
        {
            let center = divider + 1 + disks_width / 2;
            canvas.text_preserve_spaces(
                center.saturating_sub(units::display_width(label).div_ceil(2)),
                y,
                label,
                theme::MAIN,
            );
        }
        let mut row = y + 1;
        if io_row == 1 {
            if let Some(history) = app.disk_histories.get(&disk.mount) {
                draw_graph_background(
                    canvas,
                    Rect::new(divider + 6, row, disks_width.saturating_sub(6), 1),
                );
                draw_graph_options(
                    canvas,
                    Rect::new(divider + 6, row, disks_width.saturating_sub(6), 1),
                    &history.activity,
                    100.0,
                    theme::Style::Available(100),
                    false,
                    false,
                );
            }
            canvas.text(divider + 2, row, "IO%", theme::MAIN);
            if disks_width < 25
                && let Some(label) = &io_label
            {
                canvas.text_preserve_spaces(
                    area.x + area.w.saturating_sub(units::display_width(label) + 2),
                    row,
                    label,
                    theme::MAIN,
                );
            }
            row += 1;
        }
        let used = ratio(disk.used, disk.total);
        let free = ratio(disk.free, disk.total);
        canvas.text(
            divider + 2,
            row,
            &format!("Used:{used:>3.0}% "),
            theme::MAIN,
        );
        meter(
            canvas,
            divider + 12,
            row,
            meter_width,
            used,
            theme::Style::Used(100),
        );
        let used_human = units::bytes_spaced(disk.used, app.config.base_10_sizes);
        canvas.text(
            area.x + area.w.saturating_sub(used_human.len() + 2),
            row,
            &used_human,
            theme::MAIN,
        );
        row += 1;
        if show_free {
            canvas.text(
                divider + 2,
                row,
                &format!("Free:{free:>3.0}% "),
                theme::MAIN,
            );
            meter(
                canvas,
                divider + 12,
                row,
                meter_width,
                free,
                theme::Style::Free(100),
            );
            let free_human = units::bytes_spaced(disk.free, app.config.base_10_sizes);
            canvas.text(
                area.x + area.w.saturating_sub(free_human.len() + 2),
                row,
                &free_human,
                theme::MAIN,
            );
        }
        cy += disk_rows;
    }
    if cy < area.h - 2 {
        draw_disk_divider(canvas, area, divider, area.y + 1 + cy);
    }
}

fn disk_io_label(read_per_second: u64, write_per_second: u64, base_10: bool) -> Option<String> {
    let total = read_per_second.saturating_add(write_per_second);
    if total == 0 {
        return None;
    }
    let direction = match (write_per_second > 0, read_per_second > 0) {
        (true, true) => "▼▲",
        (true, false) => " ▼",
        (false, true) => " ▲",
        (false, false) => unreachable!(),
    };
    Some(format!("{direction}{}", units::bytes_short(total, base_10)))
}

fn draw_disks_io(
    canvas: &mut Canvas,
    area: Rect,
    divider: usize,
    disks_width: usize,
    app: &AppState,
    combined: bool,
) {
    let disks: Vec<_> = app
        .sample
        .memory
        .disks
        .iter()
        .filter(|disk| disk.io_supported)
        .collect();
    if disks.is_empty() {
        return;
    }
    let graph_height = area
        .h
        .saturating_sub(2 + disks.len() * 2)
        .checked_div(disks.len())
        .unwrap_or(0)
        .max(if combined { 1 } else { 2 });
    let half_height = graph_height.div_ceil(2);
    let custom_speeds = disk_io_speeds(app.config.value("io_graph_speeds").unwrap_or(""));
    let mut cy = 0usize;
    for disk in disks {
        if cy + graph_height + 1 >= area.h - 1 {
            break;
        }
        let y = area.y + 1 + cy;
        draw_disk_divider(canvas, area, divider, y);
        canvas.text_bold(divider + 2, y, &disk_name(&disk.mount), theme::TITLE);
        let total = units::bytes_spaced(disk.total, app.config.base_10_sizes);
        canvas.text_preserve_spaces_bold(
            area.x + area.w.saturating_sub(total.len() + 2),
            y,
            &total,
            theme::TITLE,
        );
        if disks_width >= 25 {
            let used = format!("{:.0}%", ratio(disk.used, disk.total));
            let center = divider + disks_width / 2;
            canvas.text(center.saturating_sub(used.len() / 2), y, &used, theme::MAIN);
        }
        let activity_y = y + 1;
        if let Some(history) = app.disk_histories.get(&disk.mount) {
            draw_graph_background(
                canvas,
                Rect::new(divider + 6, activity_y, disks_width.saturating_sub(6), 1),
            );
            draw_graph_options(
                canvas,
                Rect::new(divider + 6, activity_y, disks_width.saturating_sub(6), 1),
                &history.activity,
                100.0,
                theme::Style::Available(100),
                false,
                false,
            );
        }
        canvas.text(divider + 2, activity_y, "IO%", theme::MAIN);
        let graph_y = y + 2;
        let maximum = custom_speeds
            .get(&disk.mount)
            .copied()
            .unwrap_or(100 * 1024 * 1024) as f64;
        if let Some(history) = app.disk_histories.get(&disk.mount) {
            if combined {
                let mut both = VecDeque::with_capacity(history.read.len());
                for (read, write) in history.read.iter().zip(&history.write) {
                    both.push_back(read + write);
                }
                draw_graph_options(
                    canvas,
                    Rect::new(divider + 1, graph_y, disks_width, graph_height),
                    &both,
                    maximum,
                    theme::Style::Available(100),
                    false,
                    true,
                );
                let value = disk.read_per_second.saturating_add(disk.write_per_second);
                let label = if value == 0 {
                    "RW".to_string()
                } else {
                    format!(
                        "{}{} {}",
                        if disk.write_per_second > 0 { "▼" } else { "" },
                        if disk.read_per_second > 0 { "▲" } else { "" },
                        units::bytes_short(value, app.config.base_10_sizes)
                    )
                };
                canvas.text(divider + 2, graph_y, &label, theme::MAIN);
            } else {
                draw_graph_options(
                    canvas,
                    Rect::new(divider + 1, graph_y, disks_width, half_height),
                    &history.read,
                    maximum,
                    theme::Style::Free(100),
                    false,
                    true,
                );
                draw_graph_options(
                    canvas,
                    Rect::new(
                        divider + 1,
                        graph_y + half_height,
                        disks_width,
                        graph_height.saturating_sub(half_height),
                    ),
                    &history.write,
                    maximum,
                    theme::Style::Used(100),
                    true,
                    true,
                );
                let read = if disk.read_per_second == 0 {
                    "R".to_string()
                } else {
                    format!(
                        "▲{}",
                        units::bytes_short(disk.read_per_second, app.config.base_10_sizes)
                    )
                };
                let write = if disk.write_per_second == 0 {
                    "W".to_string()
                } else {
                    format!(
                        "▼{}",
                        units::bytes_short(disk.write_per_second, app.config.base_10_sizes)
                    )
                };
                canvas.text(divider + 2, graph_y, &read, theme::MAIN);
                canvas.text(
                    divider + 2,
                    graph_y + graph_height.saturating_sub(1),
                    &write,
                    theme::MAIN,
                );
            }
        }
        cy += graph_height + 2;
    }
    if cy < area.h - 2 {
        draw_disk_divider(canvas, area, divider, area.y + 1 + cy);
    }
}

fn disk_io_speeds(value: &str) -> HashMap<String, u64> {
    value
        .split_whitespace()
        .filter_map(|entry| {
            let (mount, speed) = entry.rsplit_once(':')?;
            let speed = speed.parse::<u64>().ok()?;
            Some((mount.to_string(), speed.saturating_mul(1024 * 1024)))
        })
        .collect()
}

fn draw_network(canvas: &mut Canvas, area: Rect, app: &mut AppState) {
    app.network_hitboxes.clear();
    if area.h < 3 || area.w < 3 {
        return;
    }
    let net = &app.sample.network;
    canvas.panel(area, "³net", theme::NET_BOX, None);
    if net.selected.is_empty() {
        if area.h > 4 {
            let swap = app
                .config
                .bool_value("swap_upload_download")
                .unwrap_or(false);
            let stats_width = if area.w > 45 { 27 } else { 19 };
            let stats_height = if area.h > 10 {
                9
            } else {
                area.h.saturating_sub(2)
            };
            let stats_x = area.x + area.w - stats_width - 1;
            let stats_y = area.y + (area.h.saturating_sub(2) / 2) - stats_height / 2 + 1;
            canvas.panel(
                Rect::new(stats_x, stats_y, stats_width, stats_height),
                if swap { "upload" } else { "download" },
                theme::BOX,
                None,
            );
            canvas.footer(
                stats_x + 2,
                stats_y + stats_height - 1,
                if swap { "download" } else { "upload" },
                theme::BOX,
            );
        }
        return;
    }
    let interface = units::truncate(&net.selected, 15);
    let selector = format!("←b {interface} n→");
    let interface_len = units::display_width(&interface);
    let selector_x = area.x + area.w.saturating_sub(interface_len + 9);
    let selector_len = units::display_width(&selector);
    canvas.control_title(
        selector_x,
        area.y,
        &selector,
        &[0, 1, selector_len - 2, selector_len - 1],
        true,
        theme::NET_BOX,
    );
    app.network_hitboxes.push(NetworkHitbox {
        y: area.y,
        start: selector_x + 1,
        end: selector_x + 4,
        action: NetworkAction::Previous,
    });
    app.network_hitboxes.push(NetworkHitbox {
        y: area.y,
        start: selector_x + 1 + selector_len - 3,
        end: selector_x + 1 + selector_len,
        action: NetworkAction::Next,
    });
    let zero_x = area.x + area.w.saturating_sub(interface_len + 15);
    canvas.control_title(
        zero_x,
        area.y,
        "zero",
        &[0],
        app.network_zero_active(),
        theme::NET_BOX,
    );
    app.network_hitboxes.push(NetworkHitbox {
        y: area.y,
        start: zero_x + 1,
        end: zero_x + 5,
        action: NetworkAction::Zero,
    });
    if area.w.saturating_sub(interface_len + 20) > 6 {
        let auto_x = area.x + area.w.saturating_sub(interface_len + 21);
        canvas.control_title(
            auto_x,
            area.y,
            "auto",
            &[0],
            app.config.net_auto,
            theme::NET_BOX,
        );
        app.network_hitboxes.push(NetworkHitbox {
            y: area.y,
            start: auto_x + 1,
            end: auto_x + 5,
            action: NetworkAction::Auto,
        });
    }
    if area.w.saturating_sub(interface_len + 20) > 13 {
        let sync_x = area.x + area.w.saturating_sub(interface_len + 27);
        canvas.control_title(
            sync_x,
            area.y,
            "sync",
            &[1],
            app.config.net_sync,
            theme::NET_BOX,
        );
        app.network_hitboxes.push(NetworkHitbox {
            y: area.y,
            start: sync_x + 1,
            end: sync_x + 5,
            action: NetworkAction::Sync,
        });
    }
    if let Some(address) = net.ipv4.as_ref().or(net.ipv6.as_ref())
        && area.w.saturating_sub(interface_len + 36) > units::display_width(address)
    {
        canvas.title(area.x + 8, area.y, address, theme::NET_BOX);
    }
    if area.h > 4 {
        let stats_width = if area.w > 45 { 27 } else { 19 };
        let stats_height = if area.h > 10 {
            9
        } else {
            area.h.saturating_sub(2)
        };
        let stats_x = area.x + area.w - stats_width - 1;
        let stats_y = area.y + (area.h.saturating_sub(2) / 2) - stats_height / 2 + 1;
        let download_height = ((area.h.saturating_sub(2)) as f64 / 2.0).round() as usize;
        let upload_height = area.h.saturating_sub(2 + download_height);
        let graph_width = stats_x.saturating_sub(area.x + 1);
        let max_down = if app.config.net_auto {
            app.network_graph_max[0]
        } else {
            configured_network_max(&app.config, "net_download")
        };
        let max_up = if app.config.net_auto {
            app.network_graph_max[1]
        } else {
            configured_network_max(&app.config, "net_upload")
        };
        let synced_max = max_down.max(max_up);
        let download_maximum = if app.config.net_sync {
            synced_max
        } else {
            max_down
        };
        let upload_maximum = if app.config.net_sync {
            synced_max
        } else {
            max_up
        };
        let swap = app
            .config
            .bool_value("swap_upload_download")
            .unwrap_or(false);
        let (top_history, top_maximum, top_style, bottom_history, bottom_maximum, bottom_style) =
            if swap {
                (
                    &app.upload_history,
                    upload_maximum,
                    theme::Style::Upload(100),
                    &app.download_history,
                    download_maximum,
                    theme::NET,
                )
            } else {
                (
                    &app.download_history,
                    download_maximum,
                    theme::NET,
                    &app.upload_history,
                    upload_maximum,
                    theme::Style::Upload(100),
                )
            };
        draw_graph_options(
            canvas,
            Rect::new(area.x + 1, area.y + 1, graph_width, download_height),
            top_history,
            top_maximum,
            top_style,
            swap,
            true,
        );
        draw_graph_options(
            canvas,
            Rect::new(
                area.x + 1,
                area.y + 1 + download_height,
                graph_width,
                upload_height,
            ),
            bottom_history,
            bottom_maximum,
            bottom_style,
            !swap,
            true,
        );
        canvas.text(
            area.x + 1,
            area.y + 1,
            &units::bytes_short(top_maximum as u64, app.config.base_10_sizes),
            theme::GRAPH_TEXT,
        );
        canvas.text(
            area.x + 1,
            area.y + area.h - 2,
            &units::bytes_short(bottom_maximum as u64, app.config.base_10_sizes),
            theme::GRAPH_TEXT,
        );
        canvas.panel(
            Rect::new(stats_x, stats_y, stats_width, stats_height),
            if swap { "upload" } else { "download" },
            theme::BOX,
            None,
        );
        canvas.footer(
            stats_x + 2,
            stats_y + stats_height - 1,
            if swap { "download" } else { "upload" },
            theme::BOX,
        );
        let download_speed = format!(
            "{}/s",
            units::bytes_spaced(net.download_per_second, app.config.base_10_sizes)
        );
        let upload_speed = format!(
            "{}/s",
            units::bytes_spaced(net.upload_per_second, app.config.base_10_sizes)
        );
        let base_10_bitrate = match app.config.value("base_10_bitrate") {
            Some(value) if value.eq_ignore_ascii_case("true") => true,
            Some(value) if value.eq_ignore_ascii_case("false") => false,
            _ => app.config.base_10_sizes,
        };
        let top_y = stats_y + 1;
        let bottom_y = stats_y + stats_height - stats_height / 2;
        if swap {
            draw_network_stat(
                canvas,
                stats_x,
                top_y,
                stats_width,
                stats_height,
                '▲',
                &upload_speed,
                net.upload_per_second,
                app.upload_top,
                net.uploaded,
                base_10_bitrate,
                app.config.base_10_sizes,
            );
            draw_network_stat(
                canvas,
                stats_x,
                bottom_y,
                stats_width,
                stats_height,
                '▼',
                &download_speed,
                net.download_per_second,
                app.download_top,
                net.downloaded,
                base_10_bitrate,
                app.config.base_10_sizes,
            );
        } else {
            draw_network_stat(
                canvas,
                stats_x,
                top_y,
                stats_width,
                stats_height,
                '▼',
                &download_speed,
                net.download_per_second,
                app.download_top,
                net.downloaded,
                base_10_bitrate,
                app.config.base_10_sizes,
            );
            draw_network_stat(
                canvas,
                stats_x,
                bottom_y,
                stats_width,
                stats_height,
                '▲',
                &upload_speed,
                net.upload_per_second,
                app.upload_top,
                net.uploaded,
                base_10_bitrate,
                app.config.base_10_sizes,
            );
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn draw_network_stat(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    stats_width: usize,
    stats_height: usize,
    arrow: char,
    speed: &str,
    bytes_per_second: u64,
    top: u64,
    total: u64,
    base_10_bitrate: bool,
    base_10_sizes: bool,
) {
    let speed_line = if stats_width >= 20 {
        let speed_bits = format!(
            "({})",
            units::bits_per_second(bytes_per_second, base_10_bitrate)
        );
        format!("{arrow} {speed:<10}{speed_bits:>13}")
    } else {
        format!("{arrow} {speed:<10}")
    };
    canvas.text(x + 1, y, &speed_line, theme::MAIN);
    if stats_height >= 8 {
        let width = if stats_width >= 20 { 18 } else { 10 };
        canvas.text(
            x + 1,
            y + 1,
            &format!(
                "{arrow} Top: {:>width$}",
                format!("({})", units::bits_per_second(top, base_10_bitrate)),
            ),
            theme::MAIN,
        );
    }
    if stats_height >= 6 {
        let width = if stats_width >= 20 { 16 } else { 8 };
        canvas.text(
            x + 1,
            y + 1 + usize::from(stats_height >= 8),
            &format!(
                "{arrow} Total: {:>width$}",
                units::bytes_spaced(total, base_10_sizes)
            ),
            theme::MAIN,
        );
    }
}

fn configured_network_max(config: &Config, name: &str) -> f64 {
    config
        .value(name)
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(10)
        .saturating_mul(1024 * 1024) as f64
        / 8.0
}

fn matches_process_filter(process: &ProcessSample, filter: &str) -> bool {
    if let Some(pattern) = filter.strip_prefix('!') {
        if pattern.is_empty() {
            return true;
        }
        return posix_regex_matches(pattern, &process.pid.to_string(), false)
            || posix_regex_matches(pattern, &process.name, false)
            || posix_regex_matches(pattern, &process.command, true)
            || posix_regex_matches(pattern, &process.user, false);
    }
    let filter = filter.to_ascii_lowercase();
    filter.is_empty()
        || process.pid.to_string().contains(&filter)
        || process.name.to_ascii_lowercase().contains(&filter)
        || process.command.to_ascii_lowercase().contains(&filter)
        || process.user.to_ascii_lowercase().contains(&filter)
}

fn posix_regex_matches(pattern: &str, value: &str, whole: bool) -> bool {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_void};

    unsafe extern "C" {
        fn regcomp(regex: *mut c_void, pattern: *const c_char, flags: c_int) -> c_int;
        fn regexec(
            regex: *const c_void,
            value: *const c_char,
            matches: usize,
            groups: *mut c_void,
            flags: c_int,
        ) -> c_int;
        fn regfree(regex: *mut c_void);
    }

    const REG_EXTENDED: c_int = 1;
    const REG_NOSUB: c_int = 4;
    let pattern = if whole {
        format!("^({pattern})$")
    } else {
        pattern.to_string()
    };
    let (Ok(pattern), Ok(value)) = (CString::new(pattern), CString::new(value)) else {
        return false;
    };
    // regex_t is opaque to Rust. This aligned storage is larger than regex_t
    // on every Linux libc target supported by this port.
    let mut regex = [0usize; 128];
    let regex_ptr = regex.as_mut_ptr().cast::<c_void>();
    if unsafe { regcomp(regex_ptr, pattern.as_ptr(), REG_EXTENDED | REG_NOSUB) } != 0 {
        return false;
    }
    let matched = unsafe { regexec(regex_ptr, value.as_ptr(), 0, std::ptr::null_mut(), 0) == 0 };
    unsafe { regfree(regex_ptr) };
    matched
}

fn tree_filter_with_descendants<'a>(
    processes: &[&'a ProcessSample],
    filter: &str,
) -> Vec<&'a ProcessSample> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes {
        if process.pid != process.parent {
            children
                .entry(process.parent)
                .or_default()
                .push(process.pid);
        }
    }
    let mut included = HashSet::new();
    let mut pending = processes
        .iter()
        .copied()
        .filter(|process| matches_process_filter(process, filter))
        .map(|process| process.pid)
        .collect::<Vec<_>>();
    while let Some(pid) = pending.pop() {
        if included.insert(pid)
            && let Some(descendants) = children.get(&pid)
        {
            for child in descendants {
                pending.push(*child);
            }
        }
    }
    processes
        .iter()
        .copied()
        .filter(|process| included.contains(&process.pid))
        .collect()
}

fn draw_process_details(
    canvas: &mut Canvas,
    area: Rect,
    app: &mut AppState,
    process: &ProcessSample,
) {
    let graph_width = (area.w / 3).max(area.w.saturating_sub(121));
    let divider = area.x + graph_width;
    let details_x = divider + 1;
    let details_width = area.x + area.w - details_x;

    canvas.title(
        area.x + 2,
        area.y,
        &process.pid.to_string(),
        theme::PROC_BOX,
    );
    let name_x = area.x + 4 + process.pid.to_string().len();
    canvas.title(
        name_x,
        area.y,
        &units::truncate(
            &process.name,
            graph_width.saturating_sub(process.pid.to_string().len() + 7),
        ),
        theme::PROC_BOX,
    );
    canvas.put(divider, area.y, '┬', theme::PROC_BOX);
    canvas.put(divider, area.y + 8, '┴', theme::PROC_BOX);
    for y in area.y + 1..area.y + 8 {
        canvas.put(divider, y, '│', theme::BOX);
    }
    for x in area.x + 1..area.x + area.w - 1 {
        if canvas.cells[(area.y + 8) * canvas.width + x].ch == ' ' {
            canvas.put(x, area.y + 8, '─', theme::BOX);
        }
    }
    canvas.put(area.x, area.y + 8, '├', theme::PROC_BOX);
    canvas.put(area.x + area.w - 1, area.y + 8, '┤', theme::PROC_BOX);
    canvas.title(area.x + 2, area.y + 8, "⁴proc", theme::PROC_BOX);

    draw_graph_options(
        canvas,
        Rect::new(area.x + 1, area.y + 1, graph_width.saturating_sub(1), 7),
        &app.detailed_cpu_history,
        100.0,
        theme::CPU,
        false,
        true,
    );
    let cpu_text = if process.cpu < 9.995 {
        format!("{:>4.2}", process.cpu)
    } else if process.cpu < 99.95 {
        format!("{:>4.1}", process.cpu)
    } else {
        format!("{:>4.0}", process.cpu)
    };
    canvas.text_bold(
        area.x + 1,
        area.y + 1,
        &format!("{cpu_text}%"),
        theme::TITLE,
    );
    for (row, letter) in ['C', 'P', 'U'].iter().enumerate() {
        canvas.put_bold(area.x + 1, area.y + 3 + row, *letter, theme::TITLE);
    }

    let alive = process.state != 'X' && process.state != 'x';
    let controls_enabled = alive && !app.process_selected;
    let mut control_x = details_x + 1;
    let mut controls = Vec::new();
    if area.w > 55 {
        controls.push(("terminate", vec![0], ProcessControlAction::Terminate));
    }
    controls.extend([
        ("kill", vec![0], ProcessControlAction::Kill),
        ("signals", vec![0], ProcessControlAction::Signals),
        ("Nice", vec![0], ProcessControlAction::Nice),
    ]);
    if area.w > 77 {
        controls.push(("Follow", vec![0], ProcessControlAction::Follow));
    }
    for (label, hotkeys, action) in controls {
        let active =
            action == ProcessControlAction::Follow && app.followed_pid == Some(process.pid);
        canvas.control_title_state(
            control_x,
            area.y,
            label,
            &hotkeys,
            active || action != ProcessControlAction::Follow,
            controls_enabled,
            theme::PROC_BOX,
        );
        if controls_enabled {
            app.process_control_hitboxes.push(ProcessControlHitbox {
                y: area.y,
                start: control_x + 1,
                end: control_x + 1 + units::display_width(label),
                action,
            });
        }
        control_x += units::display_width(label) + 2;
        if control_x >= area.x + area.w - 2 {
            break;
        }
    }
    let selected_matches = !app.process_selected
        || app.visible_pids.get(app.selected_process).copied() == Some(process.pid);
    let hide_x = area.x + area.w.saturating_sub(10);
    canvas.control_title_state(
        hide_x,
        area.y,
        "hide ↵",
        &[5],
        true,
        selected_matches,
        theme::PROC_BOX,
    );
    if selected_matches {
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: area.y,
            start: hide_x + 1,
            end: hide_x + 7,
            action: ProcessControlAction::Info,
        });
    }

    let parent = app
        .sample
        .processes
        .iter()
        .find(|parent| parent.pid == process.parent)
        .map(|parent| parent.name.clone())
        .unwrap_or_else(|| "?".into());
    let labels = [
        ("Status:", process_state(process.state).to_string()),
        ("Elapsed:", units::duration(process.elapsed_seconds)),
        (
            "IO/R:",
            units::bytes(process.read_bytes, app.config.base_10_sizes),
        ),
        (
            "IO/W:",
            units::bytes(process.write_bytes, app.config.base_10_sizes),
        ),
        ("Parent:", parent),
        ("User:", process.user.clone()),
        ("Threads:", process.threads.to_string()),
        ("Nice:", process.nice.to_string()),
    ];
    let item_count = (details_width.saturating_sub(2) / 10).clamp(1, 8);
    let item_width = details_width.saturating_sub(2) / item_count;
    for (index, (label, value)) in labels.iter().take(item_count).enumerate() {
        let x = details_x + 1 + index * item_width;
        canvas.text_bold(x, area.y + 1, &center_text(label, item_width), theme::TITLE);
        let value_style = if index == 0 && alive && process.state == 'R' {
            theme::Style::ProcMisc
        } else if !alive {
            theme::LOW
        } else {
            theme::MAIN
        };
        canvas.text(x, area.y + 2, &center_text(value, item_width), value_style);
    }

    let memory_percent = ratio(process.memory, app.sample.memory.total);
    let mut memory_percent_text = format!("{memory_percent:.2}");
    memory_percent_text.truncate(4);
    if memory_percent_text.ends_with('.') {
        memory_percent_text.pop();
    }
    let memory_label = if item_count > 4 { "Memory:" } else { "M:" };
    let memory_text = format!("{memory_label} {memory_percent_text:>4}% ");
    let memory_graph_width = details_width / 3;
    let memory_x = details_x + details_width / 3 - memory_text.len() - 1;
    canvas.text_bold(memory_x, area.y + 4, &memory_text, theme::TITLE);
    let memory_maximum = app
        .detailed_memory_history
        .iter()
        .copied()
        .fold(process.memory as f64 * 2.0, f64::max)
        .min(app.sample.memory.total as f64)
        .max(1.0);
    draw_graph(
        canvas,
        Rect::new(
            details_x + details_width / 3 - 1,
            area.y + 4,
            memory_graph_width,
            1,
        ),
        &app.detailed_memory_history,
        memory_maximum,
        theme::Style::ProcMisc,
    );
    canvas.text_bold(
        details_x + 2 * details_width / 3,
        area.y + 4,
        &units::bytes(process.memory, app.config.base_10_sizes),
        theme::TITLE,
    );

    for (row, letter) in ['C', 'M', 'D'].iter().enumerate() {
        canvas.put_bold(details_x + 1, area.y + 5 + row, *letter, theme::TITLE);
    }
    let command = sanitize_ascii_control(&process.command);
    let command_width = details_width.saturating_sub(5).max(1);
    let command_lines = units::display_width(&command)
        .div_ceil(command_width)
        .clamp(1, 3);
    for row in 0..command_lines {
        let start = row * command_width;
        let text = units::column_slice(&command, start, command_width);
        canvas.text(
            details_x + 3,
            area.y + 5 + if command_lines == 1 { 1 } else { row },
            &center_text(&text, command_width),
            theme::MAIN,
        );
    }
}

fn draw_processes(canvas: &mut Canvas, area: Rect, app: &mut AppState) {
    let selected_pid_before_reorder = app
        .process_selected
        .then(|| app.visible_pids.get(app.selected_process).copied())
        .flatten();
    app.process_hitboxes.clear();
    app.process_control_hitboxes.clear();
    app.process_scrollbar = None;
    if area.h < 3 || area.w < 3 {
        return;
    }
    let detail_offset = usize::from(area.h >= 14 && app.detailed_pid.is_some()) * 8;
    let controls_y = area.y + detail_offset;
    canvas.panel(
        area,
        if detail_offset == 0 { "⁴proc" } else { "" },
        theme::PROC_BOX,
        None,
    );
    let filter_x = area.x + 9;
    let maximum_filter = area.w.saturating_sub(66).max(6);
    let raw_filter = if app.filter_editing {
        app.filter_buffer.as_str()
    } else {
        app.config.process_filter.as_str()
    };
    let filter_text = if units::display_width(raw_filter) > maximum_filter {
        let reversed = raw_filter.chars().rev().collect::<String>();
        clip_text(&reversed, maximum_filter)
            .chars()
            .rev()
            .collect::<String>()
    } else {
        raw_filter.to_string()
    };
    let active_filter = app.filter_editing || !filter_text.is_empty();
    canvas.put(filter_x, controls_y, '┐', theme::PROC_BOX);
    if active_filter {
        canvas.put_bold(filter_x + 1, controls_y, 'f', theme::HI);
        canvas.put_bold(filter_x + 2, controls_y, ' ', theme::TITLE);
        canvas.text_bold(filter_x + 3, controls_y, &filter_text, theme::TITLE);
    } else {
        canvas.put(filter_x + 1, controls_y, 'f', theme::HI);
        canvas.text(filter_x + 2, controls_y, "ilter", theme::TITLE);
    }
    let filter_length = if active_filter {
        2 + units::display_width(&filter_text)
    } else {
        6
    };
    let mut filter_end = filter_x + 1 + filter_length;
    if app.filter_editing {
        canvas.put_underline(filter_end, controls_y, ' ', theme::TITLE);
        canvas.put_bold(filter_end + 1, controls_y, ' ', theme::HI);
        canvas.put_bold(filter_end + 2, controls_y, '↵', theme::HI);
        filter_end += 3;
    } else if !filter_text.is_empty() {
        canvas.put_bold(filter_end, controls_y, ' ', theme::HI);
        canvas.text_bold(filter_end + 1, controls_y, "del", theme::HI);
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: controls_y,
            start: filter_end + 1,
            end: filter_end + 4,
            action: ProcessControlAction::DeleteFilter,
        });
        filter_end += 4;
    }
    canvas.put(filter_end, controls_y, '┌', theme::PROC_BOX);
    app.process_control_hitboxes.push(ProcessControlHitbox {
        y: controls_y,
        start: filter_x + 1,
        end: filter_x + 1 + filter_length,
        action: ProcessControlAction::Filter,
    });
    let sorting = app.config.process_sort.label();
    let sort_len = units::display_width(sorting);
    let sort = format!("← {sorting} →");
    let sort_x = area.x + area.w.saturating_sub(sort_len + 8);
    canvas.control_title(
        sort_x,
        controls_y,
        &sort,
        &[0, sort.chars().count() - 1],
        true,
        theme::PROC_BOX,
    );
    app.process_control_hitboxes.push(ProcessControlHitbox {
        y: controls_y,
        start: sort_x + 1,
        end: sort_x + 3,
        action: ProcessControlAction::SortPrevious,
    });
    app.process_control_hitboxes.push(ProcessControlHitbox {
        y: controls_y,
        start: sort_x + sort_len + 3,
        end: sort_x + sort_len + 5,
        action: ProcessControlAction::SortNext,
    });
    if area.w > 35 + sort_len {
        let x = sort_x.saturating_sub(6);
        canvas.control_title(
            x,
            controls_y,
            "tree",
            &[3],
            app.config.process_tree,
            theme::PROC_BOX,
        );
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: controls_y,
            start: x + 1,
            end: x + 5,
            action: ProcessControlAction::Tree,
        });
    }
    if area.w > 45 + sort_len {
        let x = sort_x.saturating_sub(15);
        canvas.control_title(
            x,
            controls_y,
            "reverse",
            &[0],
            app.config.process_reversed,
            theme::PROC_BOX,
        );
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: controls_y,
            start: x + 1,
            end: x + 8,
            action: ProcessControlAction::Reverse,
        });
    }
    if area.w > 55 + sort_len {
        let x = sort_x.saturating_sub(25);
        canvas.control_title(
            x,
            controls_y,
            "per-core",
            &[4],
            app.config.process_per_core,
            theme::PROC_BOX,
        );
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: controls_y,
            start: x + 1,
            end: x + 9,
            action: ProcessControlAction::PerCore,
        });
    }
    if area.w > 60 + sort_len {
        let x = sort_x.saturating_sub(32);
        canvas.control_title(
            x,
            controls_y,
            "pause",
            &[2],
            app.config.pause_processes,
            theme::PROC_BOX,
        );
        app.process_control_hitboxes.push(ProcessControlHitbox {
            y: controls_y,
            start: x + 1,
            end: x + 6,
            action: ProcessControlAction::Pause,
        });
    }
    let filter = if app.filter_editing {
        app.filter_buffer.as_str()
    } else {
        app.config.process_filter.as_str()
    };
    let filter_kernel = app.config.bool_value("proc_filter_kernel").unwrap_or(false);
    let mut display_processes: Vec<ProcessSample> = app
        .sample
        .processes
        .iter()
        .filter(|process| !(filter_kernel && process.kernel_thread))
        .cloned()
        .collect();
    if app.config.process_tree {
        aggregate_tree_resources(
            &mut display_processes,
            app.config.bool_value("proc_aggregate").unwrap_or(false),
            &app.collapsed_processes,
        );
    }
    let eligible = display_processes.iter().collect::<Vec<_>>();
    let mut processes = if app.config.process_tree && !filter.is_empty() {
        tree_filter_with_descendants(&eligible, filter)
    } else {
        eligible
            .into_iter()
            .filter(|process| matches_process_filter(process, filter))
            .collect()
    };
    processes.sort_by(|a, b| compare_process(a, b, app.config.process_sort));
    if app.config.process_reversed {
        processes.reverse();
    } else if !app.config.process_tree && app.config.process_sort == ProcessSort::CpuLazy {
        promote_busy_processes(&mut processes);
    }
    if processes.is_empty() {
        app.visible_pids.clear();
        canvas.text(
            area.x + 2,
            controls_y + 2,
            "No matching processes",
            theme::LOW,
        );
        return;
    }
    let parent_pids: HashSet<u32> = processes
        .iter()
        .filter(|process| {
            processes.iter().any(|candidate| {
                candidate.parent == process.pid && candidate.pid != candidate.parent
            })
        })
        .map(|process| process.pid)
        .collect();
    let listed = if app.config.process_tree {
        tree_processes(
            processes,
            app.config.process_sort,
            app.config.process_reversed,
            &app.collapsed_processes,
        )
    } else {
        processes.into_iter().map(|process| (process, 0)).collect()
    };
    if let Some(followed_pid) = app.followed_pid {
        if let Some(index) = listed
            .iter()
            .position(|(process, _)| process.pid == followed_pid)
        {
            app.selected_process = index;
            app.process_selected = true;
        } else {
            app.followed_pid = None;
        }
    } else if let Some(mut selected_pid) = selected_pid_before_reorder {
        let mut seen = HashSet::new();
        while seen.insert(selected_pid) {
            if let Some(index) = listed
                .iter()
                .position(|(process, _)| process.pid == selected_pid)
            {
                app.selected_process = index;
                break;
            }
            let Some(parent) = app
                .sample
                .processes
                .iter()
                .find(|process| process.pid == selected_pid)
                .map(|process| process.parent)
            else {
                break;
            };
            selected_pid = parent;
        }
    }
    app.visible_pids = listed.iter().map(|(process, _)| process.pid).collect();

    let mut header_y = controls_y + 1;
    if area.h >= 14
        && let Some(pid) = app.detailed_pid
    {
        let process = app
            .sample
            .processes
            .iter()
            .find(|process| process.pid == pid)
            .cloned()
            .or_else(|| {
                app.detailed_process
                    .as_ref()
                    .filter(|process| process.pid == pid)
                    .cloned()
            });
        if let Some(process) = process {
            app.detailed_process = Some(process.clone());
            draw_process_details(canvas, area, app, &process);
            header_y = area.y + 9;
        } else {
            app.detailed_pid = None;
            app.detailed_process = None;
        }
    }
    let rows = area.bottom().saturating_sub(header_y + 2);
    app.selected_process = app.selected_process.min(listed.len() - 1);
    if app.process_selected {
        if app.selected_process < app.process_offset {
            app.process_offset = app.selected_process;
        }
        if app.selected_process >= app.process_offset + rows {
            app.process_offset = app.selected_process + 1 - rows;
        }
    } else {
        app.process_offset = app.process_offset.min(listed.len().saturating_sub(rows));
    }
    let user_w = if area.w < 75 { 5 } else { 10 };
    let thread_w = if area.w < 75 { 0 } else { 4 };
    // Keep btop's -1 sentinel in the width arithmetic even though Rust uses
    // zero to mean that the thread column is hidden.
    let source_thread_w = if area.w < 75 { -1 } else { 4 };
    let show_graphs = app.config.bool_value("proc_cpu_graphs").unwrap_or(true);
    let prog_w = if area.w > 70 {
        16
    } else if area.w > 55 {
        8
    } else {
        (area.w as isize - user_w as isize - source_thread_w - 33).max(1) as usize
    };
    let mut cmd_w = if area.w > 55 {
        area.w as isize - prog_w as isize - user_w as isize - source_thread_w - 33
    } else {
        -1
    };
    let mut tree_w = area.w as isize - user_w as isize - source_thread_w - 23;
    if !show_graphs {
        cmd_w += 5;
        tree_w += 5;
    }
    let cmd_w = (cmd_w > 0).then_some(cmd_w as usize);
    let tree_w = tree_w.max(8) as usize;
    let left_w = if app.config.process_tree {
        tree_w + 1
    } else {
        8 + 1 + prog_w + 1 + cmd_w.map_or(0, |width| width + 1)
    };
    if app.config.process_tree {
        canvas.text_bold(area.x + 1, header_y, "Tree:", theme::TITLE);
    } else {
        let mut header = format!("{:>8} {:<prog_w$} ", "Pid:", "Program:");
        if let Some(cmd_w) = cmd_w {
            header.push_str(&format!("{:<cmd_w$} ", "Command:"));
        }
        canvas.text_bold(area.x + 1, header_y, &header, theme::TITLE);
    }
    let columns_x = area.x + 1 + left_w;
    let memory_header = if app.config.process_mem_bytes {
        "MemB"
    } else {
        "Mem%"
    };
    if thread_w > 0 {
        canvas.text_bold(
            columns_x.saturating_sub(4),
            header_y,
            &format!(
                "Threads: {:<user_w$} {:>5} {:>10}",
                "User:", memory_header, "Cpu%"
            ),
            theme::TITLE,
        );
    } else {
        canvas.text_bold(
            columns_x,
            header_y,
            &format!("{:<user_w$} {:>5} {:>10}", "User:", memory_header, "Cpu%"),
            theme::TITLE,
        );
    }
    let process_colors = app.config.bool_value("proc_colors").unwrap_or(true);
    let process_gradient = app.config.bool_value("proc_gradient").unwrap_or(true);
    for (listed_index, (process, depth)) in listed
        .iter()
        .enumerate()
        .skip(app.process_offset)
        .take(rows)
    {
        let row = listed_index - app.process_offset;
        let y = header_y + 1 + row;
        let selected = app.process_selected && app.process_offset + row == app.selected_process;
        let followed = app.followed_pid == Some(process.pid);
        let memory = if app.config.process_mem_bytes {
            units::bytes_short(process.memory, app.config.base_10_sizes)
        } else {
            process_memory_percent(process.memory, app.sample.memory.total)
        };
        let tree_prefix = if app.config.process_tree {
            process_tree_prefix(
                &listed,
                listed_index,
                &parent_pids,
                &app.collapsed_processes,
            )
        } else {
            String::new()
        };
        let toggle_x = (app.config.process_tree && parent_pids.contains(&process.pid)).then(|| {
            let start = area.x + 1 + depth * 3;
            (start, start + 3)
        });
        app.process_hitboxes.push(ProcessHitbox {
            y,
            index: listed_index,
            pid: process.pid,
            toggle_x,
        });
        let program_prefix = format!("{tree_prefix}{} ", process.pid);
        let program_name = process.name.clone();
        let sanitized_command = sanitize_ascii_control(&process.command);
        let short_command = sanitized_command
            .split_whitespace()
            .next()
            .unwrap_or_default();
        let short_name = short_command.rsplit('/').next().unwrap_or_default();
        let (program, prefix_draw, name_draw, suffix_draw) = if app.config.process_tree {
            let command_room = tree_w.saturating_sub(
                units::display_width(&program_prefix) + units::display_width(&program_name) + 3,
            );
            let shown_command = if process.kernel_thread {
                ""
            } else if command_room > 40 {
                sanitized_command.trim()
            } else {
                short_name
            };
            let suffix =
                if !shown_command.is_empty() && shown_command != process.name && command_room > 7 {
                    format!(" ({})", clip_text(shown_command, command_room))
                } else {
                    String::new()
                };
            let prefix = clip_text(&program_prefix, tree_w);
            let name = clip_text(
                &program_name,
                tree_w.saturating_sub(units::display_width(&prefix)),
            );
            let suffix = clip_text(
                &suffix,
                tree_w.saturating_sub(units::display_width(&prefix) + units::display_width(&name)),
            );
            let combined = clip_text(&format!("{prefix}{name}{suffix}"), tree_w);
            (combined, prefix, name, suffix)
        } else {
            let name = clip_text(&program_name, prog_w);
            let command = cmd_w
                .map(|width| clip_text(&sanitized_command, width))
                .unwrap_or_default();
            let mut combined = format!("{:>8} {name:<prog_w$} ", process.pid);
            if let Some(cmd_w) = cmd_w {
                combined.push_str(&format!("{command:<cmd_w$} "));
            }
            (combined, String::new(), name, command)
        };
        let prefix_width = units::display_width(&prefix_draw);
        let name_width = units::display_width(&name_draw);
        let user = clip_text_with_plus(&process.user, user_w);
        let cpu_graph = if show_graphs { "      " } else { "" };
        let threads = process_threads(process.threads);
        let cpu = process_cpu_percent(process.cpu);
        let line = if thread_w > 0 {
            format!(
                "{program:<left_w$}{threads:>thread_w$} {user:<user_w$} {memory:>5} {cpu_graph}{cpu:>4}  "
            )
        } else {
            format!("{program:<left_w$}{user:<user_w$} {memory:>5} {cpu_graph}{cpu:>4}  ")
        };
        if selected || followed {
            canvas.fill(
                area.x + 1,
                y,
                area.w - 2,
                ' ',
                if followed {
                    theme::FOLLOWED
                } else {
                    theme::SELECTED
                },
            );
        }
        let row_style = if followed {
            theme::FOLLOWED
        } else if selected {
            theme::SELECTED
        } else {
            theme::MAIN
        };
        let line = units::truncate(&line, area.w - 2);
        if selected || followed {
            canvas.text_bold(area.x + 1, y, &line, row_style);
        } else {
            canvas.text(area.x + 1, y, &line, row_style);
        }
        if !selected && !followed {
            let distance = listed_index.abs_diff(app.selected_process);
            let program_style = process_metric_style(
                process.cpu,
                distance,
                rows,
                process_colors,
                process_gradient,
            );
            let thread_style = process_metric_style(
                process.threads as f64 / 3.0,
                distance,
                rows,
                process_colors,
                process_gradient,
            );
            let memory_style = process_metric_style(
                ratio(process.memory, app.sample.memory.total),
                distance,
                rows,
                process_colors,
                process_gradient,
            );
            let general_style = if process_gradient {
                theme::Style::Proc((distance * 100 / rows.max(1)).min(100) as u8)
            } else {
                theme::MAIN
            };
            let mut cursor = area.x + 1;
            canvas.text(cursor, y, &" ".repeat(left_w), theme::MAIN);
            if app.config.process_tree {
                canvas.text(cursor, y, &prefix_draw, general_style);
                cursor += prefix_width;
                if process_colors {
                    canvas.text(cursor, y, &name_draw, program_style);
                } else {
                    canvas.text_bold(cursor, y, &name_draw, theme::MAIN);
                }
                cursor += name_width;
                canvas.text(cursor, y, &suffix_draw, general_style);
            } else {
                canvas.text(cursor, y, &format!("{:>8} ", process.pid), general_style);
                cursor += 9;
                if process_colors {
                    canvas.text(cursor, y, &format!("{name_draw:<prog_w$}"), program_style);
                } else {
                    canvas.text_bold(cursor, y, &format!("{name_draw:<prog_w$}"), theme::MAIN);
                }
                cursor += prog_w + 1;
                if let Some(cmd_w) = cmd_w {
                    canvas.text(cursor, y, &format!("{suffix_draw:<cmd_w$}"), general_style);
                }
            }
            cursor = columns_x;
            if thread_w > 0 {
                if process_colors {
                    canvas.text(cursor, y, &format!("{threads:>thread_w$}"), thread_style);
                } else {
                    canvas.text_bold(cursor, y, &format!("{threads:>thread_w$}"), theme::MAIN);
                }
                cursor += thread_w + 1;
            }
            canvas.text(cursor, y, &format!("{user:<user_w$}"), general_style);
            cursor += user_w + 1;
            if process_colors {
                canvas.text(cursor, y, &format!("{memory:>5}"), memory_style);
            } else {
                canvas.text_bold(cursor, y, &format!("{memory:>5}"), theme::MAIN);
            }
            cursor += 6 + usize::from(show_graphs) * 6;
            if process_colors {
                canvas.text(cursor, y, &format!("{cpu:>4}"), program_style);
            } else {
                canvas.text_bold(cursor, y, &format!("{cpu:>4}"), theme::MAIN);
            }
        }
        if show_graphs && let Some(history) = app.process_cpu_histories.get(&process.pid) {
            let graph_style = process_metric_style(
                process.cpu,
                listed_index.abs_diff(app.selected_process),
                rows,
                process_colors,
                process_gradient,
            );
            let graph_x = area.x
                + 1
                + left_w
                + usize::from(thread_w > 0) * (thread_w + 1)
                + user_w
                + 1
                + 5
                + 1;
            let row_style = followed
                .then_some(theme::FOLLOWED)
                .or_else(|| selected.then_some(theme::SELECTED));
            if let Some(row_style) = row_style {
                let background = graph_background_char(canvas);
                for index in 0..5 {
                    canvas.put_bold(graph_x + index, y, background, row_style);
                }
                let mut graph = Canvas::new(5, 1);
                graph.graph_symbol = canvas.graph_symbol;
                graph.tty = canvas.tty;
                draw_graph_options(
                    &mut graph,
                    Rect::new(0, 0, 5, 1),
                    history,
                    100.0,
                    graph_style,
                    false,
                    false,
                );
                for (index, cell) in graph.cells.into_iter().enumerate() {
                    if cell.ch != ' ' {
                        canvas.put_bold(graph_x + index, y, cell.ch, row_style);
                    }
                }
            } else {
                draw_graph_background(canvas, Rect::new(graph_x, y, 5, 1));
                draw_graph_options(
                    canvas,
                    Rect::new(graph_x, y, 5, 1),
                    history,
                    100.0,
                    graph_style,
                    false,
                    false,
                );
            }
        }
    }
    let bottom = area.y + area.h - 1;
    if listed.len() > rows && rows > 0 {
        let x = area.x + area.w - 2;
        let up_y = header_y;
        let down_y = bottom.saturating_sub(1);
        let track_top = up_y + 1;
        let track_bottom = down_y;
        let track_len = track_bottom.saturating_sub(track_top);
        let maximum_offset = listed.len().saturating_sub(rows);
        let thumb_offset = if maximum_offset == 0 || track_len <= 1 {
            0
        } else {
            ((app.process_offset as f64 * (track_len - 1) as f64 / maximum_offset as f64).round()
                as usize)
                .min(track_len - 1)
        };
        let thumb_y = track_top + thumb_offset;
        canvas.put(x, up_y, '↑', theme::MAIN);
        canvas.put(x, down_y, '↓', theme::MAIN);
        for y in track_top..track_bottom {
            canvas.put(x, y, if y == thumb_y { '█' } else { ' ' }, theme::MAIN);
        }
        app.process_scrollbar = Some(ProcessScrollbar {
            x,
            up_y,
            down_y,
            track_top,
            track_bottom,
            thumb_y,
            total: listed.len(),
            visible: rows,
        });
    }
    if app.config.pause_processes || app.followed_pid.is_some() {
        let (message, style) = match (app.config.pause_processes, app.followed_pid.is_some()) {
            (true, true) => (
                "Paused list and Following process",
                theme::Style::ProcPauseFollow,
            ),
            (true, false) => ("Process list paused", theme::Style::ProcPause),
            (false, true) => ("Following process", theme::Style::ProcFollow),
            (false, false) => unreachable!(),
        };
        let y = bottom.saturating_sub(1);
        canvas.fill(area.x + 1, y, area.w.saturating_sub(2), ' ', style);
        let x = area.x + area.w.saturating_sub(units::display_width(message)) / 2;
        canvas.text_bold(x, y, message, style);
    }
    if let Some(scrollbar) = app.process_scrollbar {
        canvas.put_bold(scrollbar.x, scrollbar.up_y, '↑', theme::MAIN);
        canvas.put_bold(scrollbar.x, scrollbar.down_y, '↓', theme::MAIN);
        canvas.put_bold(scrollbar.x, scrollbar.thumb_y, '█', theme::MAIN);
    }
    let mut button_x = area.x + 1;
    canvas.put(button_x, bottom, '┘', theme::PROC_BOX);
    canvas.put_bold(
        button_x + 1,
        bottom,
        '↑',
        if app.process_selected {
            theme::HI
        } else {
            theme::LOW
        },
    );
    canvas.text_bold(button_x + 2, bottom, " select ", theme::TITLE);
    canvas.put_bold(
        button_x + 10,
        bottom,
        '↓',
        if !app.process_selected || app.selected_process + 1 < listed.len() {
            theme::HI
        } else {
            theme::LOW
        },
    );
    canvas.put(button_x + 11, bottom, '└', theme::PROC_BOX);
    button_x += 12;
    let mut buttons = vec![("info ↵", Some(ProcessControlAction::Info), vec![5], true)];
    if area.w > 60 {
        buttons.push((
            "terminate",
            Some(ProcessControlAction::Terminate),
            vec![0],
            true,
        ));
    }
    if area.w > 55 {
        buttons.push(("kill", Some(ProcessControlAction::Kill), vec![0], true));
    }
    buttons.extend([
        (
            "signals",
            Some(ProcessControlAction::Signals),
            vec![0],
            true,
        ),
        ("Nice", Some(ProcessControlAction::Nice), vec![0], true),
    ]);
    if area.w > 72 {
        buttons.push((
            "Follow",
            Some(ProcessControlAction::Follow),
            vec![0],
            app.followed_pid.is_some(),
        ));
    }
    for (button, action, hotkeys, active) in buttons {
        canvas.control_footer_state(
            button_x,
            bottom,
            button,
            &hotkeys,
            active,
            action.is_none() || app.process_selected,
            theme::PROC_BOX,
        );
        if app.process_selected
            && let Some(action) = action
        {
            app.process_control_hitboxes.push(ProcessControlHitbox {
                y: bottom,
                start: button_x + 1,
                end: button_x + 1 + units::display_width(button),
                action,
            });
        }
        button_x += units::display_width(button) + 2;
    }
    let location = format!(
        "{}/{}",
        if app.process_selected {
            app.selected_process + 1
        } else {
            0
        },
        listed.len()
    );
    canvas.footer(
        area.x + area.w.saturating_sub(location.len() + 3),
        bottom,
        &location,
        theme::PROC_BOX,
    );
}

fn clip_text(text: &str, width: usize) -> String {
    let mut used = 0;
    text.chars()
        .take_while(|character| {
            let next = used + units::char_width(*character);
            if next > width {
                false
            } else {
                used = next;
                true
            }
        })
        .collect()
}

fn center_text(text: &str, width: usize) -> String {
    let text = clip_text(text, width);
    let padding = width.saturating_sub(units::display_width(&text));
    format!(
        "{}{}{}",
        " ".repeat(padding.div_ceil(2)),
        text,
        " ".repeat(padding / 2)
    )
}

// btop calculates centered terminal coordinates in its one-based coordinate
// system. Even-width objects therefore land one cell left of Rust's usual
// `(total - width) / 2` result after conversion to our zero-based canvas.
fn source_center_x(total: usize, width: usize) -> usize {
    total
        .saturating_div(2)
        .saturating_sub(width.saturating_div(2) + 1)
}

fn sanitize_ascii_control(text: &str) -> String {
    text.chars()
        .map(|character| {
            if character.is_ascii_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

fn clip_text_with_plus(text: &str, width: usize) -> String {
    if units::display_width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    clip_text(text, width - 1) + "+"
}

fn process_threads(threads: u32) -> String {
    if threads > 9_999 {
        format!("{}K", threads / 1_000)
    } else {
        threads.to_string()
    }
}

fn process_cpu_percent(cpu: f64) -> String {
    let mut text = format!("{cpu:.2}");
    if cpu < 10.0 || (100.0..1_000.0).contains(&cpu) {
        text.truncate(3);
    } else if cpu >= 10_000.0 {
        text = format!("{:.2}", cpu / 1_000.0);
        text.truncate(3);
        if text.ends_with('.') {
            text.pop();
        }
        text.push('k');
    }
    text
}

fn process_memory_percent(memory: u64, total: u64) -> String {
    let percent = ratio(memory, total).clamp(0.0, 100.0);
    let mut text = if percent < 0.01 {
        "0".to_string()
    } else {
        format!("{percent:.1}")
    };
    if text.len() > 3 {
        text.truncate(3);
    }
    if text.ends_with('.') {
        text.pop();
    }
    text.push('%');
    text
}

fn process_metric_style(
    metric: f64,
    distance: usize,
    rows: usize,
    colors: bool,
    gradient: bool,
) -> theme::Style {
    if !colors {
        return theme::MAIN;
    }
    let metric = metric.round().clamp(0.0, 100.0) as i32;
    if !gradient {
        return theme::Style::Process(metric as u8);
    }
    let value = metric + 100 - (distance * 100 / rows.max(1)) as i32;
    if value < 100 {
        theme::Style::ProcColor(value.clamp(0, 100) as u8)
    } else {
        theme::Style::Process((value - 100).clamp(0, 100) as u8)
    }
}

fn process_state(state: char) -> &'static str {
    match state {
        'R' => "Running",
        'S' => "Sleeping",
        'D' => "Disk sleep",
        'Z' => "Zombie",
        'T' | 't' => "Stopped",
        'X' | 'x' => "Dead",
        'I' => "Idle",
        'P' => "Parked",
        _ => "Unknown",
    }
}

fn draw_banner(canvas: &mut Canvas, y: usize) {
    const BANNER: [&str; 6] = [
        "██████╗ ████████╗ ██████╗ ██████╗",
        "██╔══██╗╚══██╔══╝██╔═══██╗██╔══██╗   ██╗    ██╗",
        "██████╔╝   ██║   ██║   ██║██████╔╝ ██████╗██████╗",
        "██╔══██╗   ██║   ██║   ██║██╔═══╝  ╚═██╔═╝╚═██╔═╝",
        "██████╔╝   ██║   ╚██████╔╝██║        ╚═╝    ╚═╝",
        "╚═════╝    ╚═╝    ╚═════╝ ╚═╝",
    ];
    let width = BANNER
        .iter()
        .map(|line| units::display_width(line))
        .max()
        .unwrap_or(0);
    let x = source_center_x(canvas.width, width);
    for (line_no, line) in BANNER.iter().enumerate() {
        for (offset, ch) in line.chars().enumerate() {
            if ch != ' ' {
                canvas.put(
                    x + offset,
                    y + line_no,
                    ch,
                    if ch == '█' {
                        theme::Style::Banner(line_no as u8)
                    } else {
                        theme::Style::BannerGray(line_no as u8)
                    },
                );
            }
        }
    }
    canvas.text_bold_italic(x + 42, y + 5, "v1.4.7", theme::MAIN);
}

fn draw_no_boxes(canvas: &mut Canvas) {
    let banner_y = canvas.height.saturating_div(2).saturating_sub(11);
    draw_banner(canvas, banner_y);
    let x = canvas.width.saturating_div(2).saturating_sub(11);
    canvas.text_bold(x, banner_y + 6, "No boxes shown!", theme::TITLE);
    for (row, (key, description)) in [
        ("1", "Show CPU box"),
        ("2", "Show MEM box"),
        ("3", "Show NET box"),
        ("4", "Show PROC box"),
        ("5-0", "Show GPU boxes"),
        ("esc", "Show menu"),
        ("q", "Quit"),
    ]
    .iter()
    .enumerate()
    {
        canvas.text_bold(x.saturating_sub(2), banner_y + 8 + row, key, theme::HI);
        canvas.text_bold(
            x.saturating_sub(2) + units::display_width(key),
            banner_y + 8 + row,
            &format!(" | {description}"),
            theme::MAIN,
        );
    }
}

fn draw_main_menu(canvas: &mut Canvas, app: &mut AppState, selected: u8) {
    app.main_menu_hitboxes.clear();
    const NORMAL: [[&str; 3]; 3] = [
        [
            "┌─┐┌─┐┌┬┐┬┌─┐┌┐┌┌─┐",
            "│ │├─┘ │ ││ ││││└─┐",
            "└─┘┴   ┴ ┴└─┘┘└┘└─┘",
        ],
        ["┬ ┬┌─┐┬  ┌─┐", "├─┤├┤ │  ├─┘", "┴ ┴└─┘┴─┘┴  "],
        ["┌─┐ ┬ ┬ ┬┌┬┐", "│─┼┐│ │ │ │ ", "└─┘└└─┘ ┴ ┴ "],
    ];
    const SELECTED: [[&str; 3]; 3] = [
        [
            "╔═╗╔═╗╔╦╗╦╔═╗╔╗╔╔═╗",
            "║ ║╠═╝ ║ ║║ ║║║║╚═╗",
            "╚═╝╩   ╩ ╩╚═╝╝╚╝╚═╝",
        ],
        ["╦ ╦╔═╗╦  ╔═╗", "╠═╣╠╣ ║  ╠═╝", "╩ ╩╚═╝╩═╝╩  "],
        ["╔═╗ ╦ ╦ ╦╔╦╗ ", "║═╬╗║ ║ ║ ║  ", "╚═╝╚╚═╝ ╩ ╩  "],
    ];
    let banner_y = canvas.height.saturating_div(2).saturating_sub(11);
    draw_banner(canvas, banner_y);
    let mut y = banner_y + 7;
    for item in 0..3 {
        let art = if !canvas.tty && selected as usize == item {
            &SELECTED[item]
        } else {
            &NORMAL[item]
        };
        let item_y = y;
        let item_width = art
            .iter()
            .map(|line| units::display_width(line))
            .max()
            .unwrap_or(0);
        app.main_menu_hitboxes.push(MainMenuHitbox {
            item: item as u8,
            x: source_center_x(canvas.width, item_width),
            y: item_y,
            width: item_width,
            height: art.len(),
        });
        for (line_no, line) in art.iter().enumerate() {
            let x = source_center_x(canvas.width, units::display_width(line));
            canvas.text_bold(
                x,
                y,
                line,
                if canvas.tty && selected as usize == item {
                    theme::HI
                } else if canvas.tty {
                    theme::MAIN
                } else if selected as usize == item {
                    theme::Style::Banner((line_no * 2) as u8)
                } else {
                    theme::Style::MenuNormal(line_no as u8)
                },
            );
            y += 1;
        }
    }
}

fn draw_help(canvas: &mut Canvas, page: usize) {
    const HELP: [(&str, &str); 46] = [
        ("Mouse 1", "Clicks buttons and selects in process list."),
        (
            "Mouse scroll",
            "Scrolls any scrollable list/text under cursor.",
        ),
        ("Esc, m", "Toggles main menu."),
        ("p", "Cycle view presets forwards."),
        ("shift + p", "Cycle view presets backwards."),
        ("1", "Toggle CPU box."),
        ("2", "Toggle MEM box."),
        ("3", "Toggle NET box."),
        ("4", "Toggle PROC box."),
        ("5", "Toggle GPU box."),
        ("d", "Toggle disks view in MEM box."),
        ("F2, o", "Shows options."),
        ("F1, ?, h", "Shows this window."),
        ("ctrl + z", "Sleep program and put in background."),
        ("ctrl + r", "Reloads config file from disk."),
        ("q, ctrl + c", "Quits program."),
        ("+, -", "Add/Subtract 100ms to/from update timer."),
        ("Up, Down", "Select in process list."),
        ("Enter", "Show detailed information for selected process."),
        (
            "Spacebar",
            "Expand/collapse the selected process in tree view.",
        ),
        ("C", "Expand/collapse the selected process' children."),
        ("Pg Up, Pg Down", "Jump 1 page in process list."),
        ("Home, End", "Jump to first or last page in process list."),
        ("Left, Right", "Select previous/next sorting column."),
        ("b, n", "Select previous/next network device."),
        ("i", "Toggle disks io mode with big graphs."),
        ("z", "Toggle totals reset for current network device"),
        ("a", "Toggle auto scaling for the network graphs."),
        ("y", "Toggle synced scaling mode for network graphs."),
        ("f, /", "To enter a process filter. Start with ! for regex."),
        ("F", "Follow selected process."),
        ("u", "Pause process list."),
        ("delete", "Clear any entered filter."),
        ("c", "Toggle per-core cpu usage of processes."),
        ("r", "Reverse sorting order in processes box."),
        ("e", "Toggle processes tree view."),
        ("E", "Collapse/expand all processes in tree view."),
        ("%", "Toggles memory display mode in processes box."),
        (
            "Selected +, -",
            "Expand/collapse the selected process in tree view.",
        ),
        (
            "Selected t",
            "Terminate selected process with SIGTERM - 15.",
        ),
        ("Selected k", "Kill selected process with SIGKILL - 9."),
        ("Selected s", "Select or enter signal to send to process."),
        ("Selected N", "Select new nice value for selected process."),
        ("", " "),
        ("", "For bug reporting and project updates, visit:"),
        ("", "https://github.com/aristocratos/btop"),
    ];
    let panel_width = 78.min(canvas.width.saturating_sub(2));
    let height = canvas.height.saturating_sub(6).min(HELP.len() + 3);
    let banner_y = canvas
        .height
        .saturating_div(2)
        .saturating_sub(5 + HELP.len() / 2);
    draw_banner(canvas, banner_y);
    let area = Rect::new(
        source_center_x(canvas.width, panel_width),
        banner_y + 6,
        panel_width,
        height,
    );
    canvas.shadow(area);
    canvas.panel(area, "help", theme::HI, None);
    canvas.text_bold(
        area.x + 1,
        area.y + 1,
        &format!("{}Description:", center_text("Key:", 20)),
        theme::TITLE,
    );
    let per_page = height.saturating_sub(3);
    for (row, (key, description)) in HELP.iter().skip(page * per_page).take(per_page).enumerate() {
        let y = area.y + 2 + row;
        canvas.text_bold(area.x + 1, y, &center_text(key, 20), theme::HI);
        canvas.text(area.x + 21, y, description, theme::MAIN);
    }
    if HELP.len() > per_page {
        let label = format!("↑ page {}/2 ↓", page + 1);
        canvas.control_footer(
            area.x + 2,
            area.y + area.h - 1,
            &label,
            &[0, label.chars().count() - 1],
            true,
            theme::HI,
        );
    }
}

fn is_integer_option(option: &str) -> bool {
    matches!(
        option,
        "update_ms" | "net_download" | "net_upload" | "proc_tree_auto_collapse"
    )
}

fn option_has_arrows(option: &str, app: &AppState) -> bool {
    app.config.bool_value(option).is_some()
        || is_integer_option(option)
        || option == "color_theme"
        || dynamic_option_choices(option, app).is_some()
        || option_choices(option).is_some()
}

fn option_is_editable(option: &str, app: &AppState) -> bool {
    is_integer_option(option)
        || (app.config.bool_value(option).is_none()
            && option != "color_theme"
            && dynamic_option_choices(option, app).is_none()
            && option_choices(option).is_none())
}

fn dynamic_option_choices(option: &str, app: &AppState) -> Option<Vec<String>> {
    match option {
        "cpu_sensor" => Some(temperature_sensor_names()),
        "selected_battery" => Some(app.sample.cpu.available_batteries.clone()),
        "cpu_graph_upper" | "cpu_graph_lower" => {
            let mut choices = vec!["Auto".to_string(), "total".to_string()];
            let mut fields = app.sample.cpu.fields.keys().cloned().collect::<Vec<_>>();
            fields.sort();
            choices.extend(fields);
            if !app.sample.gpus.is_empty() {
                choices.extend([
                    "gpu-totals".into(),
                    "gpu-vram-totals".into(),
                    "gpu-pwr-totals".into(),
                ]);
            }
            Some(choices)
        }
        _ => None,
    }
}

fn option_choices(option: &str) -> Option<&'static [&'static str]> {
    match option {
        "disable_presets" => Some(&["Off", "Default", "Custom", "All"]),
        "graph_symbol" => Some(&["braille", "block", "tty"]),
        "graph_symbol_cpu" | "graph_symbol_gpu" | "graph_symbol_mem" | "graph_symbol_net"
        | "graph_symbol_proc" => Some(&["default", "braille", "block", "tty"]),
        "show_gpu_info" => Some(&["Auto", "On", "Off"]),
        "temp_scale" => Some(&["celsius", "fahrenheit", "kelvin", "rankine"]),
        "freq_mode" => Some(&["first", "range", "lowest", "highest", "average"]),
        "log_level" => Some(&["DISABLED", "ERROR", "WARNING", "INFO", "DEBUG"]),
        "proc_sorting" => Some(&[
            "pid",
            "name",
            "command",
            "threads",
            "user",
            "memory",
            "cpu direct",
            "cpu lazy",
        ]),
        "base_10_bitrate" => Some(&["Auto", "True", "False"]),
        _ => None,
    }
}

fn cycle_option<T: AsRef<str>>(config: &mut Config, option: &str, choices: &[T], direction: i32) {
    if choices.is_empty() {
        return;
    }
    let current = config.value(option).unwrap_or_default();
    let position = choices.iter().position(|choice| choice.as_ref() == current);
    let next = match (position, direction > 0) {
        (Some(index), true) => (index + 1) % choices.len(),
        (Some(index), false) => (index + choices.len() - 1) % choices.len(),
        (None, true) => 0,
        (None, false) => choices.len() - 1,
    };
    config.set_value(option, choices[next].as_ref());
}

fn cycle_theme(config: &mut Config, choices: &[String], direction: i32) {
    if choices.is_empty() {
        return;
    }
    let current = std::path::Path::new(config.value("color_theme").unwrap_or_default());
    let position = choices.iter().position(|choice| {
        let candidate = std::path::Path::new(choice);
        candidate == current
            || candidate.file_name() == current.file_name()
            || candidate.file_stem() == current.file_stem()
    });
    let next = match (position, direction > 0) {
        (Some(index), true) => (index + 1) % choices.len(),
        (Some(index), false) => (index + choices.len() - 1) % choices.len(),
        (None, true) => 0,
        (None, false) => choices.len() - 1,
    };
    config.set_value("color_theme", &choices[next]);
}

fn theme_choices(config: &Config) -> Vec<String> {
    theme::available_themes(config.themes_dir.as_deref())
}

fn draw_options(
    canvas: &mut Canvas,
    app: &AppState,
    category: usize,
    page: usize,
    selected: usize,
) {
    let options = OPTION_CATEGORIES[category.min(OPTION_CATEGORIES.len() - 1)];
    let width = 78.min(canvas.width.saturating_sub(2));
    let max_items = OPTION_CATEGORIES
        .iter()
        .map(|category| category.len())
        .max()
        .unwrap_or(1);
    let height = canvas.height.saturating_sub(7).min(max_items * 2 + 4) & !1;
    let banner_y = canvas
        .height
        .saturating_div(2)
        .saturating_sub(4 + max_items);
    draw_banner(canvas, banner_y);
    let area = Rect::new(
        source_center_x(canvas.width, width),
        banner_y + 6,
        width,
        height,
    );
    canvas.shadow(area);
    canvas.panel(area, "", theme::HI, None);
    canvas.put(area.x + 2, area.y, '┐', theme::HI);
    canvas.text_bold(area.x + 3, area.y, "tab", theme::HI);
    canvas.put_bold(area.x + 6, area.y, '→', theme::MAIN);
    canvas.put(area.x + 7, area.y, '┌', theme::HI);
    let labels = ["general", "cpu", "gpu", "mem", "net", "proc"];
    let mut category_x = area.x + 4;
    for (index, label) in labels.iter().enumerate() {
        let text = if index == category {
            format!("[{label}]")
        } else {
            format!("{}{label} ", index + 1)
        };
        canvas.text_bold(category_x, area.y + 1, &text, theme::TITLE);
        canvas.put_bold(
            category_x,
            area.y + 1,
            text.chars().next().unwrap_or(' '),
            theme::HI,
        );
        if index == category {
            canvas.put_bold(
                category_x + units::display_width(&text) - 1,
                area.y + 1,
                ']',
                theme::HI,
            );
        }
        category_x += units::display_width(&text) + 7;
    }
    canvas.put(area.x, area.y + 2, '├', theme::HI);
    for x in area.x + 1..area.x + area.w - 1 {
        canvas.put(x, area.y + 2, '─', theme::BOX);
    }
    canvas.put(area.x + 30, area.y + 2, '┬', theme::BOX);
    canvas.put(area.x + area.w - 1, area.y + 2, '┤', theme::HI);
    for y in area.y + 3..area.y + area.h - 1 {
        canvas.put(area.x + 30, y, '│', theme::BOX);
    }
    canvas.put(area.x + 30, area.y + area.h - 1, '┴', theme::HI);

    let per_page = (height.saturating_sub(4) / 2).max(1);
    let start = page * per_page;
    for (row, option) in options.iter().skip(start).take(per_page).enumerate() {
        let y = area.y + 3 + row * 2;
        let selected_row = row == selected;
        if selected_row {
            canvas.fill(area.x + 1, y, 29, ' ', theme::SELECTED);
            canvas.fill(area.x + 1, y + 1, 29, ' ', theme::SELECTED);
        }
        let name = option.replace('_', " ");
        let index_suffix = selected_row
            .then(|| option_choice_index(option, app))
            .flatten()
            .map(|(index, count)| format!(" {}/{}", index + 1, count))
            .unwrap_or_default();
        canvas.text_bold(
            area.x + 1,
            y,
            &center_text(&format!("{}{index_suffix}", capitalize(&name)), 29),
            if selected_row {
                theme::SELECTED
            } else {
                theme::TITLE
            },
        );
        let editing_value =
            (selected_row && app.options_editing).then(|| clip_text(&app.options_buffer, 24));
        let value = editing_value
            .clone()
            .unwrap_or_else(|| option_value(option, app));
        canvas.text(
            area.x + 1,
            y + 1,
            &format!(
                "  {}  ",
                if editing_value.is_some() {
                    center_text(&format!("{value} "), 25)
                } else {
                    center_text(&value, 25)
                }
            ),
            if selected_row {
                theme::SELECTED
            } else {
                theme::MAIN
            },
        );
        if let Some(editing_value) = editing_value {
            let content_width = units::display_width(&editing_value) + 1;
            let left_padding = 25usize.saturating_sub(content_width).div_ceil(2);
            canvas.put_underline(
                area.x + 3 + left_padding + units::display_width(&editing_value),
                y + 1,
                ' ',
                theme::SELECTED,
            );
        }
        if selected_row && !app.options_editing && option_has_arrows(option, app) {
            canvas.put_bold(area.x + 2, y + 1, '←', theme::SELECTED);
            canvas.put_bold(area.x + 28, y + 1, '→', theme::SELECTED);
        }
        if selected_row && option_is_editable(option, app) {
            canvas.put_bold(
                area.x + if is_integer_option(option) { 26 } else { 28 },
                y + 1,
                '↵',
                theme::SELECTED,
            );
        }
        if selected_row {
            let description = option_description(option);
            for (line, text) in description.iter().enumerate() {
                if area.y + 3 + line >= area.y + area.h - 1 {
                    break;
                }
                if line == 0 {
                    canvas.text_bold(area.x + 32, area.y + 3, text, theme::TITLE);
                } else {
                    canvas.text(area.x + 32, area.y + 3 + line, text, theme::MAIN);
                    if *option == "disks_filter" && text.starts_with("Prepend exclude=") {
                        canvas.text_italic(area.x + 40, area.y + 3 + line, "exclude=", theme::MAIN);
                    }
                }
            }
        }
    }
    if options.len() > per_page {
        let pages = options.len().div_ceil(per_page);
        let label = format!("↑ page {}/{} ↓", page.min(pages - 1) + 1, pages);
        canvas.control_footer(
            area.x + 2,
            area.y + area.h - 1,
            &label,
            &[0, label.chars().count() - 1],
            true,
            theme::HI,
        );
    }
}

fn option_choice_index(option: &str, app: &AppState) -> Option<(usize, usize)> {
    let choices = if option == "color_theme" {
        theme_choices(&app.config)
    } else if let Some(choices) = dynamic_option_choices(option, app) {
        choices
    } else {
        option_choices(option)?
            .iter()
            .map(|choice| (*choice).to_string())
            .collect()
    };
    let current = option_value(option, app);
    let index = if option == "color_theme" {
        let current = std::path::Path::new(&app.config.color_theme);
        choices.iter().position(|choice| {
            let candidate = std::path::Path::new(choice);
            candidate == current
                || candidate.file_name() == current.file_name()
                || candidate.file_stem() == current.file_stem()
        })
    } else {
        choices.iter().position(|choice| choice == &current)
    }
    .unwrap_or(choices.len());
    Some((index, choices.len()))
}

fn capitalize(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default()
}

fn option_value(option: &str, app: &AppState) -> String {
    if let Some(value) = app.config.bool_value(option) {
        return if value { "True" } else { "False" }.into();
    }
    match option {
        "color_theme" => std::path::Path::new(&app.config.color_theme)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Default")
            .into(),
        "shown_boxes" => app.config.value("shown_boxes").unwrap_or_default().into(),
        "update_ms" => app.config.update_ms.to_string(),
        "graph_symbol" => match app.config.graph_symbol {
            GraphSymbol::Braille => "braille",
            GraphSymbol::Block => "block",
            GraphSymbol::Tty => "tty",
        }
        .into(),
        "graph_symbol_cpu" | "graph_symbol_mem" | "graph_symbol_net" | "graph_symbol_proc"
        | "graph_symbol_gpu" => app.config.value(option).unwrap_or("default").into(),
        "clock_format" => app.config.clock_format.clone(),
        "net_iface" => app.config.net_iface.clone().unwrap_or_default(),
        "proc_sorting" => app.config.process_sort.label().into(),
        _ => app.config.value(option).unwrap_or_default().into(),
    }
}

fn option_description(option: &str) -> &'static [&'static str] {
    match option {
        "color_theme" => &[
            "Set color theme.",
            "",
            "Choose from all theme files in (usually)",
            "\"/usr/[local/]share/btop/themes\" and",
            "\"~/.config/btop/themes\".",
            "",
            "\"Default\" for builtin default theme.",
            "\"TTY\" for builtin 16-color theme.",
            "",
            "For theme updates see:",
            "https://github.com/aristocratos/btop",
        ],
        "theme_background" => &[
            "If the theme set background should be shown.",
            "",
            "Set to False if you want terminal background",
            "transparency.",
        ],
        "truecolor" => &[
            "Sets if 24-bit truecolor should be used.",
            "",
            "Will convert 24-bit colors to 256 color",
            "(6x6x6 color cube) if False.",
            "",
            "Set to False if your terminal doesn't have",
            "truecolor support and can't convert to",
            "256-color.",
        ],
        "force_tty" => &[
            "TTY mode.",
            "",
            "Set to true to force tty mode regardless",
            "if a real tty has been detected or not.",
            "",
            "Will force 16-color mode and TTY theme,",
            "set all graph symbols to \"tty\" and swap",
            "out other non tty friendly symbols.",
        ],
        "vim_keys" => &[
            "Enable vim keys.",
            "Set to True to enable \"h,j,k,l\" keys for",
            "directional control in lists.",
            "",
            "Conflicting keys for",
            "h (help) and k (kill)",
            "is accessible while holding shift.",
        ],
        "disable_mouse" => &["Disable all mouse events."],
        "disable_presets" => &[
            "Disable the presets.",
            "",
            "\"Off\" All presets are enabled.",
            "",
            "\"Default\" preset is disabled.",
            "",
            "\"Custom\" presets are disabled.",
            "",
            "\"All\" presets are disabled.",
        ],
        "presets" => &[
            "Define presets for the layout of the boxes.",
            "",
            "Preset 0 is always all boxes shown with",
            "default settings.",
            "Max 9 presets.",
            "",
            "Format: \"box_name:P:G,box_name:P:G\"",
            "P=(0 or 1) for alternate positions.",
            "G=graph symbol to use for box.",
            "",
            "Use whitespace \" \" as separator between",
            "different presets.",
            "",
            "Example:",
            "\"mem:0:tty,proc:1:default cpu:0:braille\"",
        ],
        "shown_boxes" => &[
            "Manually set which boxes to show.",
            "",
            "Available values are \"cpu mem net proc\".",
            "Or \"gpu0\" through \"gpu5\" for GPU boxes.",
            "Separate values with whitespace.",
            "",
            "Toggle between presets with key \"p\".",
        ],
        "update_ms" => &[
            "Update time in milliseconds.",
            "",
            "Recommended 2000 ms or above for better",
            "sample times for graphs.",
            "",
            "Min value: 100 ms",
            "Max value: 86400000 ms = 24 hours.",
        ],
        "rounded_corners" => &[
            "Rounded corners on boxes.",
            "",
            "True or False",
            "",
            "Is always False if TTY mode is ON.",
        ],
        "terminal_sync" => &[
            "Output synchronization.",
            "",
            "Use terminal synchronized output sequences",
            "to reduce flickering on supported terminals.",
            "",
            "True or False.",
        ],
        "graph_symbol" => &[
            "Default symbols to use for graph creation.",
            "",
            "\"braille\", \"block\" or \"tty\".",
            "",
            "\"braille\" offers the highest resolution but",
            "might not be included in all fonts.",
            "",
            "\"block\" has half the resolution of braille",
            "but uses more common characters.",
            "",
            "\"tty\" uses only 3 different symbols but will",
            "work with most fonts.",
            "",
            "Note that \"tty\" only has half the horizontal",
            "resolution of the other two,",
            "so will show a shorter historical view.",
        ],
        "clock_format" => &[
            "Draw a clock at top of screen.",
            "(Only visible if cpu box is enabled!)",
            "",
            "Formatting according to strftime, empty",
            "string to disable.",
            "",
            "Custom formatting options:",
            "\"/host\" = hostname",
            "\"/user\" = username",
            "\"/uptime\" = system uptime",
            "",
            "Examples of strftime formats:",
            "\"%X\" = locale HH:MM:SS",
            "\"%H\" = 24h hour, \"%I\" = 12h hour",
            "\"%M\" = minute, \"%S\" = second",
            "\"%d\" = day, \"%m\" = month, \"%y\" = year",
        ],
        "base_10_sizes" => &[
            "Use base 10 for bits and bytes sizes.",
            "",
            "Uses KB = 1000 instead of KiB = 1024,",
            "MB = 1000KB instead of MiB = 1024KiB,",
            "and so on.",
            "",
            "True or False.",
        ],
        "background_update" => &[
            "Update main ui when menus are showing.",
            "",
            "True or False.",
            "",
            "Set this to false if the menus is flickering",
            "too much for a comfortable experience.",
        ],
        "show_battery" => &[
            "Show battery stats.",
            "(Only visible if cpu box is enabled!)",
            "",
            "Show battery stats in the top right corner",
            "if a battery is present.",
        ],
        "selected_battery" => &[
            "Select battery.",
            "",
            "Which battery to use if multiple are present.",
            "Can be both batteries and UPS.",
            "",
            "\"Auto\" for auto detection.",
        ],
        "show_battery_watts" => &[
            "Show battery power.",
            "",
            "Show discharge power when discharging.",
            "Show charging power when charging.",
        ],
        "log_level" => &[
            "Set loglevel for error.log",
            "",
            "\"ERROR\", \"WARNING\", \"INFO\" and \"DEBUG\".",
            "",
            "The level set includes all lower levels,",
            "i.e. \"DEBUG\" will show all logging info.",
        ],
        "save_config_on_exit" => &[
            "Save config on exit.",
            "",
            "Automatically save current settings to",
            "config file on exit.",
            "",
            "When this is toggled from True to False",
            "a save is immediately triggered.",
            "This way a manual save can be done by",
            "toggling this setting on and off again.",
        ],
        "cpu_bottom" => &[
            "Cpu box location.",
            "",
            "Show cpu box at bottom of screen instead",
            "of top.",
        ],
        "graph_symbol_cpu" => &[
            "Graph symbol to use for graphs in cpu box.",
            "",
            "\"default\", \"braille\", \"block\" or \"tty\".",
            "",
            "\"default\" for the general default symbol.",
        ],
        "cpu_graph_upper" => &[
            "Cpu upper graph.",
            "",
            "Sets the CPU/GPU stat shown in upper half of",
            "the CPU graph.",
            "",
            "CPU:",
            "\"total\" = Total cpu usage. (Auto)",
            "\"user\" = User mode cpu usage.",
            "\"system\" = Kernel mode cpu usage.",
            "+ more depending on kernel.",
            "",
            "GPU:",
            "\"gpu-totals\" = GPU usage split by device.",
            "\"gpu-vram-totals\" = VRAM usage split by GPU.",
            "\"gpu-pwr-totals\" = Power usage split by GPU.",
            "\"gpu-average\" = Avg usage of all GPUs.",
            "\"gpu-vram-total\" = VRAM usage of all GPUs.",
            "\"gpu-pwr-total\" = Power usage of all GPUs.",
            "Not all stats are supported on all devices.",
        ],
        "cpu_graph_lower" => &[
            "Cpu lower graph.",
            "",
            "Sets the CPU/GPU stat shown in lower half of",
            "the CPU graph.",
            "",
            "CPU:",
            "\"total\" = Total cpu usage.",
            "\"user\" = User mode cpu usage.",
            "\"system\" = Kernel mode cpu usage.",
            "+ more depending on kernel.",
            "",
            "GPU:",
            "\"gpu-totals\" = GPU usage split/device. (Auto)",
            "\"gpu-vram-totals\" = VRAM usage split by GPU.",
            "\"gpu-pwr-totals\" = Power usage split by GPU.",
            "\"gpu-average\" = Avg usage of all GPUs.",
            "\"gpu-vram-total\" = VRAM usage of all GPUs.",
            "\"gpu-pwr-total\" = Power usage of all GPUs.",
            "Not all stats are supported on all devices.",
        ],
        "cpu_invert_lower" => &[
            "Toggles orientation of the lower CPU graph.",
            "",
            "True or False.",
        ],
        "cpu_single_graph" => &[
            "Completely disable the lower CPU graph.",
            "",
            "Shows only upper CPU graph and resizes it",
            "to fit to box height.",
            "",
            "True or False.",
        ],
        "show_gpu_info" => &[
            "Show gpu info in cpu box.",
            "",
            "Toggles gpu stats in cpu box and the",
            "gpu graph (if \"cpu_graph_lower\" is set to",
            "\"Auto\").",
            "",
            "\"Auto\" to show when no gpu box is shown.",
            "\"On\" to always show.",
            "\"Off\" to never show.",
        ],
        "check_temp" => &["Enable cpu temperature reporting.", "", "True or False."],
        "cpu_sensor" => &[
            "Cpu temperature sensor.",
            "",
            "Select the sensor that corresponds to",
            "your cpu temperature.",
            "",
            "Set to \"Auto\" for auto detection.",
        ],
        "show_coretemp" => &[
            "Show temperatures for cpu cores.",
            "",
            "Only works if check_temp is True and",
            "the system is reporting core temps.",
        ],
        "cpu_core_map" => &[
            "Custom mapping between core and coretemp.",
            "",
            "Can be needed on certain cpus to get correct",
            "temperature for correct core.",
            "",
            "Use lm-sensors or similar to see which cores",
            "are reporting temperatures on your machine.",
            "",
            "Format: \"X:Y\"",
            "X=core with wrong temp.",
            "Y=core with correct temp.",
            "Use space as separator between multiple",
            "entries.",
            "",
            "Example: \"4:0 5:1 6:3\"",
        ],
        "temp_scale" => &[
            "Which temperature scale to use.",
            "",
            "Celsius, default scale.",
            "",
            "Fahrenheit, the american one.",
            "",
            "Kelvin, 0 = absolute zero, 1 degree change",
            "equals 1 degree change in Celsius.",
            "",
            "Rankine, 0 = absolute zero, 1 degree change",
            "equals 1 degree change in Fahrenheit.",
        ],
        "show_cpu_freq" => &[
            "Show CPU frequency.",
            "",
            "Can cause slowdowns on systems with many",
            "cores and certain kernel versions.",
        ],
        "freq_mode" => &[
            "How the CPU frequency will be displayed.",
            "",
            "First, get the frequency from the first",
            "core.",
            "",
            "Range, show the lowest and the highest",
            "frequency.",
            "",
            "Lowest, the lowest frequency.",
            "",
            "Highest, the highest frequency.",
            "",
            "Average, sum and divide.",
        ],
        "custom_cpu_name" => &[
            "Custom cpu model name in cpu percentage box.",
            "",
            "Empty string to disable.",
        ],
        "show_uptime" => &[
            "Shows the system uptime in the CPU box.",
            "",
            "Can also be shown in the clock by using",
            "\"/uptime\" in the formatting.",
            "",
            "True or False.",
        ],
        "show_cpu_watts" => &[
            "Shows the CPU power consumption in watts.",
            "",
            "Requires running `make setcap` or",
            "`make setuid` or running with sudo.",
            "",
            "True or False.",
        ],
        "nvml_measure_pcie_speeds" => &[
            "Measure PCIe throughput on NVIDIA cards.",
            "",
            "May impact performance on certain cards.",
            "",
            "True or False.",
        ],
        "rsmi_measure_pcie_speeds" => &[
            "Measure PCIe throughput on AMD cards.",
            "",
            "May impact performance on certain cards.",
            "",
            "True or False.",
        ],
        "graph_symbol_gpu" => &[
            "Graph symbol to use for graphs in gpu box.",
            "",
            "\"default\", \"braille\", \"block\" or \"tty\".",
            "",
            "\"default\" for the general default symbol.",
        ],
        "gpu_mirror_graph" => &["Horizontally mirror the GPU graph.", "", "True or False."],
        "shown_gpus" => &[
            "Manually set which gpu vendors to show.",
            "",
            "Available values are",
            "\"nvidia\", \"amd\", \"intel\",",
            "and \"apple\".",
            "Separate values with whitespace.",
            "",
            "A restart is required to apply changes.",
        ],
        "custom_gpu_name0" => &[
            "Custom gpu0 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "custom_gpu_name1" => &[
            "Custom gpu1 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "custom_gpu_name2" => &[
            "Custom gpu2 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "custom_gpu_name3" => &[
            "Custom gpu3 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "custom_gpu_name4" => &[
            "Custom gpu4 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "custom_gpu_name5" => &[
            "Custom gpu5 model name in gpu stats box.",
            "",
            "Empty string to disable.",
        ],
        "mem_below_net" => &[
            "Mem box location.",
            "",
            "Show mem box below net box instead of above.",
        ],
        "graph_symbol_mem" => &[
            "Graph symbol to use for graphs in mem box.",
            "",
            "\"default\", \"braille\", \"block\" or \"tty\".",
            "",
            "\"default\" for the general default symbol.",
        ],
        "mem_graphs" => &["Show graphs for memory values.", "", "True or False."],
        "show_disks" => &["Split memory box to also show disks.", "", "True or False."],
        "show_io_stat" => &[
            "Toggle IO activity graphs.",
            "",
            "Show small IO graphs that for disk activity",
            "(disk busy time) when not in IO mode.",
            "",
            "True or False.",
        ],
        "io_mode" => &[
            "Toggles io mode for disks.",
            "",
            "Shows big graphs for disk read/write speeds",
            "instead of used/free percentage meters.",
            "",
            "True or False.",
        ],
        "io_graph_combined" => &[
            "Toggle combined read and write graphs.",
            "",
            "Only has effect if \"io mode\" is True.",
            "",
            "True or False.",
        ],
        "io_graph_speeds" => &[
            "Set top speeds for the io graphs.",
            "",
            "Manually set which speed in MiB/s that",
            "equals 100 percent in the io graphs.",
            "(100 MiB/s by default).",
            "",
            "Format: \"device:speed\" separate disks with",
            "whitespace \" \".",
            "",
            "Example: \"/dev/sda:100, /dev/sdb:20\".",
        ],
        "show_swap" => &[
            "If swap memory should be shown in memory box.",
            "",
            "True or False.",
        ],
        "swap_disk" => &[
            "Show swap as a disk.",
            "",
            "Ignores show_swap value above.",
            "Inserts itself after first disk.",
        ],
        "only_physical" => &[
            "Filter out non physical disks.",
            "",
            "Set this to False to include network disks,",
            "RAM disks and similar.",
            "",
            "True or False.",
        ],
        "use_fstab" => &[
            "(Linux) Read disks list from /etc/fstab.",
            "",
            "This also disables only_physical.",
            "",
            "True or False.",
        ],
        "zfs_hide_datasets" => &[
            "(Linux) Hide ZFS datasets in disks list.",
            "",
            "Setting this to True will hide all datasets,",
            "and only show ZFS pools.",
            "",
            "(IO stats will be calculated per-pool)",
            "",
            "True or False.",
        ],
        "disk_free_priv" => &[
            "(Linux) Type of available disk space.",
            "",
            "Set to true to show how much disk space is",
            "available for privileged users.",
            "",
            "Set to false to show available for normal",
            "users.",
        ],
        "disks_filter" => &[
            "Optional filter for shown disks.",
            "",
            "Should be full path of a mountpoint.",
            "Separate multiple values with",
            "whitespace \" \".",
            "",
            "Only disks matching the filter will be shown.",
            "Prepend exclude= to only show disks ",
            "not matching the filter.",
            "",
            "Examples:",
            "/boot /home/user",
            "exclude=/boot /home/user",
        ],
        "zfs_arc_cached" => &[
            "(Linux) Count ZFS ARC as cached memory.",
            "",
            "Add ZFS ARC used to cached memory and",
            "ZFS ARC available to available memory.",
            "These are otherwise reported by the Linux",
            "kernel as used memory.",
            "",
            "True or False.",
        ],
        "graph_symbol_net" => &[
            "Graph symbol to use for graphs in net box.",
            "",
            "\"default\", \"braille\", \"block\" or \"tty\".",
            "",
            "\"default\" for the general default symbol.",
        ],
        "swap_upload_download" => &[
            "Swap the positions of the upload and download",
            "graphs.",
            "",
            "This allows for a more \"intuitive\" view",
            "with download being down, on the bottom.",
        ],
        "net_download" => &[
            "Fixed network graph download value.",
            "",
            "Value in Mebibits, default \"100\".",
            "",
            "Can be toggled with auto button.",
        ],
        "net_upload" => &[
            "Fixed network graph upload value.",
            "",
            "Value in Mebibits, default \"100\".",
            "",
            "Can be toggled with auto button.",
        ],
        "net_auto" => &[
            "Start in network graphs auto rescaling mode.",
            "",
            "Ignores any values set above at start and",
            "rescales down to 10Kibibytes at the lowest.",
            "",
            "True or False.",
        ],
        "net_sync" => &[
            "Network scale sync.",
            "",
            "Syncs the scaling for download and upload to",
            "whichever currently has the highest scale.",
            "",
            "True or False.",
        ],
        "net_iface" => &[
            "Network Interface.",
            "",
            "Manually set the starting Network Interface.",
            "",
            "Will otherwise automatically choose the NIC",
            "with the highest total download since boot.",
        ],
        "base_10_bitrate" => &[
            "Base 10 bitrate",
            "",
            "True:  Use SI prefixes for bitrates",
            "       (1000Kbps = 1Mbps)",
            "False: Use binary prefixes for bitrates",
            "       (1024Kibps = 1Mibps)",
            "Auto:  Use the General -> Base 10 Sizes",
            "       setting for bitrates",
            "",
            "True, False, or Auto",
        ],
        "proc_left" => &[
            "Proc box location.",
            "",
            "Show proc box on left side of screen",
            "instead of right.",
        ],
        "graph_symbol_proc" => &[
            "Graph symbol to use for graphs in proc box.",
            "",
            "\"default\", \"braille\", \"block\" or \"tty\".",
            "",
            "\"default\" for the general default symbol.",
        ],
        "proc_sorting" => &[
            "Processes sorting option.",
            "",
            "Possible values:",
            "\"pid\", \"program\", \"arguments\", \"threads\",",
            "\"user\", \"memory\", \"cpu lazy\" and",
            "\"cpu direct\".",
            "",
            "\"cpu lazy\" updates top process over time.",
            "\"cpu direct\" updates top process",
            "directly.",
        ],
        "proc_reversed" => &["Reverse processes sorting order.", "", "True or False."],
        "proc_tree" => &[
            "Processes tree view.",
            "",
            "Set true to show processes grouped by",
            "parents with lines drawn between parent",
            "and child process.",
        ],
        "proc_aggregate" => &[
            "Aggregate child's resources in parent.",
            "",
            "In tree-view, include all child resources",
            "with the parent even while expanded.",
        ],
        "proc_tree_auto_collapse" => &[
            "Auto-collapse busy parents in tree view.",
            "",
            "When entering tree mode, automatically",
            "collapse any process that has this many",
            "or more direct children.",
            "",
            "Useful for hiding noisy multi-process apps",
            "like Chrome, Firefox or Electron.",
            "",
            "Set to 0 to disable.",
            "",
            "Min value: 0",
            "Max value: 10000",
        ],
        "proc_colors" => &["Enable colors in process view.", "", "True or False."],
        "proc_gradient" => &[
            "Enable process view gradient fade.",
            "",
            "Fades from top or current selection.",
            "Max fade value is equal to current themes",
            "\"inactive_fg\" color value.",
        ],
        "proc_per_core" => &[
            "Process usage per core.",
            "",
            "If process cpu usage should be of the core",
            "it's running on or usage of the total",
            "available cpu power.",
            "",
            "If true and process is multithreaded",
            "cpu usage can reach over 100%.",
        ],
        "proc_mem_bytes" => &[
            "Show memory as bytes in process list.",
            " ",
            "Will show percentage of total memory",
            "if False.",
        ],
        "keep_dead_proc_usage" => &[
            "Cpu and Mem usage for dead processes",
            "",
            "Set true if process should preserve the cpu",
            "and memory usage of when it died while",
            "paused.",
        ],
        "proc_cpu_graphs" => &["Show cpu graph for each process.", "", "True or False"],
        "proc_filter_kernel" => &[
            "(Linux) Filter kernel processes from output.",
            "",
            "Set to 'True' to filter out internal",
            "processes started by the Linux kernel.",
        ],
        "proc_follow_detailed" => &[
            "Follow selected process with detailed view",
            "",
            "If set to 'True' then when opening the",
            "detailed view, the process will be",
            "followed in the list. Pressing enter",
            "again will close the detailed view",
            "and stop following the process.",
        ],
        _ => &["Use Left/Right to change this option."],
    }
}

fn move_signal_horizontal(selected: u8, right: bool) -> u8 {
    let mut next = i32::from(selected);
    if right {
        next += 1;
        if next > 31 {
            next = 1;
        } else if next == 16 {
            next = 17;
        }
    } else {
        next -= 1;
        if next < 1 {
            next = 31;
        } else if next == 16 {
            next = 15;
        }
    }
    next as u8
}

fn move_signal_vertical(selected: u8, down: bool) -> u8 {
    let mut next = i32::from(selected);
    if down {
        if next == 31 || next < 1 || next == 16 {
            next = 1;
        } else if next > 26 {
            next -= 25;
        } else {
            let below_gap = next < 16;
            next += 5;
            if next >= 16 && below_gap {
                next += 1;
            }
            next = next.min(31);
        }
    } else if next != 16 {
        if next == 1 {
            next = 31;
        } else if next < 6 {
            next += 25;
        } else {
            let above_gap = next > 16;
            next -= 5;
            if next <= 16 && above_gap {
                next -= 1;
            }
        }
    }
    next as u8
}

fn draw_signal(canvas: &mut Canvas, app: &mut AppState, pid: u32, signal: i32) {
    app.signal_confirm_hitboxes.clear();
    let w = 50.min(canvas.width.saturating_sub(4));
    let h = 9.min(canvas.height.saturating_sub(2));
    let area = Rect::new(
        source_center_x(canvas.width, w),
        (canvas.height - h) / 2,
        w,
        h,
    );
    canvas.shadow(area);
    let name = if signal == 15 { "SIGTERM" } else { "SIGKILL" };
    canvas.panel(area, name, theme::RED, None);
    let process_name = app
        .sample
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .map(|process| units::truncate(&process.name, 16))
        .unwrap_or_default();
    let signal_text = signal.to_string();
    let signal_line = format!("Send signal: {signal_text} ({name})");
    let signal_x = area.x
        + 1
        + (area
            .w
            .saturating_sub(2 + units::display_width(&signal_line)))
        .div_ceil(2);
    canvas.text_bold(signal_x, area.y + 2, "Send signal: ", theme::MAIN);
    canvas.text(signal_x + 13, area.y + 2, &signal_text, theme::HI);
    canvas.text(
        signal_x + 13 + signal_text.len(),
        area.y + 2,
        &format!(" ({name})"),
        theme::MAIN,
    );
    let pid_text = pid.to_string();
    let pid_line = format!("To PID: {pid_text} ({process_name})");
    let pid_x =
        area.x + 1 + (area.w.saturating_sub(2 + units::display_width(&pid_line))).div_ceil(2);
    canvas.text_bold(pid_x, area.y + 3, "To PID: ", theme::MAIN);
    canvas.text(pid_x + 8, area.y + 3, &pid_text, theme::HI);
    canvas.text(
        pid_x + 8 + pid_text.len(),
        area.y + 3,
        &format!(" ({process_name})"),
        theme::MAIN,
    );
    let yes = Rect::new(area.x + area.w / 2 - 13, area.y + 5, 13, 3);
    let no = Rect::new(area.x + area.w / 2 + 2, area.y + 5, 12, 3);
    draw_dialog_button(canvas, yes, "Yes", true);
    draw_dialog_button(canvas, no, "No", false);
    app.signal_confirm_hitboxes.push((true, yes));
    app.signal_confirm_hitboxes.push((false, no));
}

fn draw_dialog_button(canvas: &mut Canvas, area: Rect, label: &str, selected: bool) {
    let line_style = if selected { theme::HI } else { theme::BOX };
    canvas.panel(area, "", line_style, None);
    let x = area.x + area.w.saturating_sub(units::display_width(label)) / 2;
    if selected {
        canvas.text_bold(x, area.y + 1, label, theme::TITLE);
    } else {
        canvas.text(x, area.y + 1, label, theme::MAIN);
    }
}

fn draw_signal_chooser(canvas: &mut Canvas, app: &mut AppState, pid: u32, selected: u8) {
    app.signal_choice_hitboxes.clear();
    const SIGNALS: [&str; 32] = [
        "0",
        "SIGHUP",
        "SIGINT",
        "SIGQUIT",
        "SIGILL",
        "SIGTRAP",
        "SIGABRT",
        "SIGBUS",
        "SIGFPE",
        "SIGKILL",
        "SIGUSR1",
        "SIGSEGV",
        "SIGUSR2",
        "SIGPIPE",
        "SIGALRM",
        "SIGTERM",
        "SIGSTKFLT",
        "SIGCHLD",
        "SIGCONT",
        "SIGSTOP",
        "SIGTSTP",
        "SIGTTIN",
        "SIGTTOU",
        "SIGURG",
        "SIGXCPU",
        "SIGXFSZ",
        "SIGVTALRM",
        "SIGPROF",
        "SIGWINCH",
        "SIGIO",
        "SIGPWR",
        "SIGSYS",
    ];
    let width = 78.min(canvas.width.saturating_sub(4));
    let height = 19.min(canvas.height.saturating_sub(2));
    let area = Rect::new(
        canvas.width.saturating_sub(width) / 2,
        canvas.height.saturating_sub(height) / 2,
        width,
        height,
    );
    canvas.shadow(area);
    canvas.panel(area, "signals", theme::HI, None);
    let process_name = app
        .sample
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .map(|process| units::truncate(&process.name, 30))
        .unwrap_or_default();
    canvas.text_bold(
        area.x + 1,
        area.y + 2,
        &center_text(&format!("Send signal to PID {pid} ({process_name})"), 76),
        theme::TITLE,
    );
    let signal_value = if app.signal_buffer.is_empty() {
        if selected > 0 {
            selected.to_string()
        } else {
            String::new()
        }
    } else {
        app.signal_buffer.clone()
    };
    canvas.text(
        area.x + 1,
        area.y + 4,
        &format!("{:>48}", "Enter signal number: "),
        theme::MAIN,
    );
    canvas.text(area.x + 49, area.y + 4, &signal_value, theme::HI);
    canvas.put_blink(
        area.x + 49 + units::display_width(&signal_value),
        area.y + 4,
        '█',
        theme::MAIN,
    );
    for (ordinal, (index, signal)) in SIGNALS
        .iter()
        .enumerate()
        .filter(|(index, _)| *index != 0 && *index != 16)
        .enumerate()
    {
        let column = ordinal % 5;
        let row = ordinal / 5;
        let x = area.x + 2 + column * 15;
        let y = area.y + 6 + row;
        if index == selected as usize {
            canvas.fill(x, y, 15, ' ', theme::SELECTED);
        }
        if index == selected as usize {
            canvas.text_bold(
                x,
                y,
                &format!("{index:<3}{:<12}", format!("({signal})")),
                theme::SELECTED,
            );
        } else {
            canvas.text(x, y, &format!("{index:<3}"), theme::HI);
            canvas.text(
                x + 3,
                y,
                &format!("{:<12}", format!("({signal})")),
                theme::MAIN,
            );
        }
        app.signal_choice_hitboxes
            .push((index as u8, Rect::new(x, y, 15, 1)));
    }
    for (row, (key, description)) in [
        ("↑ ↓ ← →", "To choose signal."),
        ("0-9", "Enter manually."),
        ("ENTER", "To send signal."),
        ("ESC or \"q\"", "To abort."),
    ]
    .iter()
    .enumerate()
    {
        canvas.text_bold(
            area.x + 1,
            area.y + 13 + row,
            &format!("{key:>33}"),
            theme::HI,
        );
        canvas.text(
            area.x + 34,
            area.y + 13 + row,
            &format!(" | {description}"),
            theme::MAIN,
        );
    }
}

fn draw_renice(canvas: &mut Canvas, app: &AppState, pid: u32, value: i32) {
    let width = 50.min(canvas.width.saturating_sub(4));
    let height = 13.min(canvas.height.saturating_sub(2));
    let area = Rect::new(
        if width == 50 {
            canvas.width.saturating_div(2).saturating_sub(24)
        } else {
            canvas.width.saturating_sub(width) / 2
        },
        canvas.height.saturating_sub(height) / 2,
        width,
        height,
    );
    canvas.shadow(area);
    canvas.panel(area, "renice", theme::HI, None);
    let process_name = app
        .sample
        .processes
        .iter()
        .find(|process| process.pid == pid)
        .map(|process| units::truncate(&process.name, 15))
        .unwrap_or_default();
    canvas.text_bold(
        area.x + 1,
        area.y + 2,
        &center_text(&format!("Renice PID {pid} ({process_name})"), 48),
        theme::TITLE,
    );
    let nice_value = if app.renice_buffer.is_empty() {
        value.to_string()
    } else {
        app.renice_buffer.clone()
    };
    canvas.text(
        area.x + 1,
        area.y + 4,
        &format!("{:>30}", "Enter nice value: "),
        theme::MAIN,
    );
    canvas.text(area.x + 31, area.y + 4, &nice_value, theme::HI);
    canvas.put_blink(
        area.x + 31 + units::display_width(&nice_value),
        area.y + 4,
        '█',
        theme::MAIN,
    );
    for (row, (key, description)) in [
        ("↑ ↓", "To change value."),
        ("← →", "To change value by 5."),
        ("0-9", "Enter manually."),
        ("ENTER", "To set nice value."),
        ("ESC or 'q'", "To abort."),
    ]
    .iter()
    .enumerate()
    {
        canvas.text_bold(
            area.x + 1,
            area.y + 7 + row,
            &format!("{key:>20}"),
            theme::HI,
        );
        canvas.text(
            area.x + 21,
            area.y + 7 + row,
            &format!(" | {description}"),
            theme::MAIN,
        );
    }
}

fn operation_result(operation: Operation, result: Result<(), i32>) -> Overlay {
    match result {
        Ok(()) => Overlay::None,
        Err(errno) => Overlay::OperationError { operation, errno },
    }
}

fn draw_operation_error(canvas: &mut Canvas, operation: Operation, errno: i32) {
    let width = 50.min(canvas.width.saturating_sub(4));
    let height = 9.min(canvas.height.saturating_sub(2));
    let area = Rect::new(
        source_center_x(canvas.width, width),
        canvas.height.saturating_sub(height) / 2,
        width,
        height,
    );
    let detail = match errno {
        22 => "Unsupported signal!".to_string(),
        1 | 13 => format!(
            "Insufficient permissions to {}!",
            if operation == Operation::Signal {
                "send signal"
            } else {
                "renice process"
            }
        ),
        3 => "Process not found!".to_string(),
        _ => format!("Unknown error! (errno: {errno})"),
    };
    canvas.shadow(area);
    canvas.panel(area, "error", theme::RED, None);
    canvas.text_bold(
        area.x + width.saturating_sub(8) / 2,
        area.y + 2,
        "Failure:",
        theme::Style::Used(100),
    );
    canvas.text(
        area.x + width.saturating_sub(units::display_width(&detail)) / 2,
        area.y + 3,
        &detail,
        theme::MAIN,
    );
    draw_dialog_button(
        canvas,
        Rect::new(area.x + width / 2 - 5, area.y + height - 4, 12, 3),
        "Ok",
        true,
    );
}

fn last_errno() -> i32 {
    std::io::Error::last_os_error().raw_os_error().unwrap_or(-1)
}

fn send_signal(pid: u32, signal: i32) -> Result<(), i32> {
    unsafe extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }
    if unsafe { kill(pid as i32, signal) } == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

fn set_nice(pid: u32, value: i32) -> Result<(), i32> {
    unsafe extern "C" {
        fn setpriority(which: i32, who: u32, priority: i32) -> i32;
    }
    if unsafe { setpriority(0, pid, value) } == 0 {
        Ok(())
    } else {
        Err(last_errno())
    }
}

fn compare_process(a: &&ProcessSample, b: &&ProcessSample, sort: ProcessSort) -> Ordering {
    match sort {
        ProcessSort::CpuDirect => b
            .cpu
            .partial_cmp(&a.cpu)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::CpuLazy => b
            .cumulative_cpu
            .partial_cmp(&a.cumulative_cpu)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::Memory => b.memory.cmp(&a.memory).then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::Pid => b.pid.cmp(&a.pid),
        ProcessSort::Name => a
            .name
            .to_ascii_lowercase()
            .cmp(&b.name.to_ascii_lowercase()),
        ProcessSort::Command => a
            .command
            .to_ascii_lowercase()
            .cmp(&b.command.to_ascii_lowercase()),
        ProcessSort::User => a.user.cmp(&b.user).then_with(|| a.pid.cmp(&b.pid)),
        ProcessSort::Threads => b.threads.cmp(&a.threads).then_with(|| a.pid.cmp(&b.pid)),
    }
}

// btop's lazy sort is cumulative CPU order with a small live-CPU exception:
// move at most eleven processes whose current use exceeds the leading group
// ahead of otherwise long-running processes.
fn promote_busy_processes(processes: &mut Vec<&ProcessSample>) {
    let mut maximum = 10.0_f64;
    let mut target = 30.0_f64;
    let mut moved = 0usize;
    let mut offset = 0usize;
    let mut index = 0usize;
    while index < processes.len() {
        if index <= 5 && processes[index].cpu > maximum {
            maximum = processes[index].cpu;
        } else if index == 6 {
            target = if maximum > 30.0 { maximum } else { 10.0 };
        }
        if index == offset && processes[index].cpu > 30.0 {
            offset += 1;
        } else if processes[index].cpu > target {
            let process = processes.remove(index);
            processes.insert(offset, process);
            moved += 1;
            if moved > 10 {
                break;
            }
        }
        index += 1;
    }
}

#[derive(Clone, Copy, Default)]
struct ProcessResources {
    cpu: f64,
    cumulative_cpu: f64,
    memory: u64,
    threads: u32,
}

fn auto_collapse_oversized(
    processes: &[ProcessSample],
    threshold: usize,
    collapsed: &mut HashSet<u32>,
) {
    if threshold == 0 || processes.is_empty() {
        return;
    }
    let root_parent = processes[0].parent;
    let root_pids = processes
        .iter()
        .filter(|process| process.parent == root_parent)
        .map(|process| process.pid)
        .collect::<HashSet<_>>();
    let mut child_counts = HashMap::new();
    for process in processes {
        *child_counts.entry(process.parent).or_insert(0usize) += 1;
    }
    for process in processes {
        if process.parent == root_parent || root_pids.contains(&process.parent) {
            continue;
        }
        if child_counts.get(&process.pid).copied().unwrap_or(0) >= threshold {
            collapsed.insert(process.pid);
        }
    }
}

fn aggregate_tree_resources(
    processes: &mut [ProcessSample],
    aggregate_all: bool,
    collapsed: &HashSet<u32>,
) {
    let by_pid = processes
        .iter()
        .map(|process| (process.pid, process.clone()))
        .collect::<HashMap<_, _>>();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for process in processes.iter() {
        if process.pid != process.parent {
            children
                .entry(process.parent)
                .or_default()
                .push(process.pid);
        }
    }
    fn total(
        pid: u32,
        by_pid: &HashMap<u32, ProcessSample>,
        children: &HashMap<u32, Vec<u32>>,
        visiting: &mut HashSet<u32>,
        memo: &mut HashMap<u32, ProcessResources>,
    ) -> ProcessResources {
        if let Some(total) = memo.get(&pid) {
            return *total;
        }
        let Some(process) = by_pid.get(&pid) else {
            return ProcessResources::default();
        };
        if !visiting.insert(pid) {
            return ProcessResources::default();
        }
        let mut resources = ProcessResources {
            cpu: process.cpu,
            cumulative_cpu: process.cumulative_cpu,
            memory: process.memory,
            threads: process.threads,
        };
        if let Some(group) = children.get(&pid) {
            for child_pid in group {
                if by_pid
                    .get(child_pid)
                    .is_some_and(|child| child.state == 'X')
                {
                    continue;
                }
                let child = total(*child_pid, by_pid, children, visiting, memo);
                resources.cpu += child.cpu;
                resources.cumulative_cpu += child.cumulative_cpu;
                resources.memory = resources.memory.saturating_add(child.memory);
                resources.threads = resources.threads.saturating_add(child.threads);
            }
        }
        visiting.remove(&pid);
        memo.insert(pid, resources);
        resources
    }

    let mut memo = HashMap::new();
    for process in processes.iter_mut() {
        if aggregate_all || collapsed.contains(&process.pid) {
            let resources = total(
                process.pid,
                &by_pid,
                &children,
                &mut HashSet::new(),
                &mut memo,
            );
            process.cpu = resources.cpu;
            process.cumulative_cpu = resources.cumulative_cpu;
            process.memory = resources.memory;
            process.threads = resources.threads;
        }
    }
}

fn tree_processes<'a>(
    processes: Vec<&'a ProcessSample>,
    sort: ProcessSort,
    reversed: bool,
    collapsed: &HashSet<u32>,
) -> Vec<(&'a ProcessSample, usize)> {
    let pids: HashSet<u32> = processes.iter().map(|process| process.pid).collect();
    let mut children: HashMap<u32, Vec<&ProcessSample>> = HashMap::new();
    let mut roots = Vec::new();
    for process in processes {
        if process.parent == process.pid || !pids.contains(&process.parent) {
            roots.push(process);
        } else {
            children.entry(process.parent).or_default().push(process);
        }
    }
    sort_process_group(&mut roots, sort, reversed);
    for group in children.values_mut() {
        sort_process_group(group, sort, reversed);
    }

    fn append<'a>(
        process: &'a ProcessSample,
        depth: usize,
        children: &HashMap<u32, Vec<&'a ProcessSample>>,
        collapsed: &HashSet<u32>,
        visited: &mut HashSet<u32>,
        output: &mut Vec<(&'a ProcessSample, usize)>,
    ) {
        if !visited.insert(process.pid) {
            return;
        }
        output.push((process, depth));
        if collapsed.contains(&process.pid) {
            mark_descendants(process.pid, children, visited);
            return;
        }
        if let Some(group) = children.get(&process.pid) {
            for child in group {
                append(child, depth + 1, children, collapsed, visited, output);
            }
        }
    }

    fn mark_descendants(
        pid: u32,
        children: &HashMap<u32, Vec<&ProcessSample>>,
        visited: &mut HashSet<u32>,
    ) {
        if let Some(group) = children.get(&pid) {
            for child in group {
                if visited.insert(child.pid) {
                    mark_descendants(child.pid, children, visited);
                }
            }
        }
    }

    let mut output = Vec::new();
    let mut visited = HashSet::new();
    for root in roots {
        append(root, 0, &children, collapsed, &mut visited, &mut output);
    }
    for group in children.values() {
        for process in group {
            append(process, 0, &children, collapsed, &mut visited, &mut output);
        }
    }
    output
}

fn process_tree_prefix(
    listed: &[(&ProcessSample, usize)],
    index: usize,
    parent_pids: &HashSet<u32>,
    collapsed: &HashSet<u32>,
) -> String {
    let (process, depth) = listed[index];
    if parent_pids.contains(&process.pid) {
        let indent = " │ ".repeat(depth);
        let marker = if collapsed.contains(&process.pid) {
            "[+]"
        } else {
            "[-]"
        };
        return format!("{indent}{marker}─");
    }
    if depth == 0 {
        return String::new();
    }
    let has_later_sibling = listed[index + 1..]
        .iter()
        .take_while(|(_, next_depth)| *next_depth >= depth)
        .any(|(_, next_depth)| *next_depth == depth);
    format!(
        "{} {}",
        " │ ".repeat(depth),
        if has_later_sibling {
            "├─"
        } else {
            "└─"
        }
    )
}

fn sort_process_group(group: &mut Vec<&ProcessSample>, sort: ProcessSort, reversed: bool) {
    group.sort_by(|a, b| compare_process(a, b, sort));
    if reversed {
        group.reverse();
    }
}

fn draw_graph_background(canvas: &mut Canvas, area: Rect) {
    let ch = graph_background_char(canvas);
    for y in area.y..area.y + area.h {
        for x in area.x..area.x + area.w {
            canvas.put(x, y, ch, theme::LOW);
        }
    }
}

fn graph_background_char(canvas: &Canvas) -> char {
    match canvas.graph_symbol {
        GraphSymbol::Braille if !canvas.tty => '⣀',
        GraphSymbol::Block if !canvas.tty => '▄',
        _ => '░',
    }
}

fn bold_area(canvas: &mut Canvas, area: Rect) {
    for y in area.y..(area.y + area.h).min(canvas.height) {
        for x in area.x..(area.x + area.w).min(canvas.width) {
            canvas.cells[y * canvas.width + x].bold = true;
        }
    }
}

fn draw_graph(
    canvas: &mut Canvas,
    area: Rect,
    history: &VecDeque<f64>,
    maximum: f64,
    style: theme::Style,
) {
    draw_graph_options(canvas, area, history, maximum, style, false, false);
}

fn draw_graph_offset(
    canvas: &mut Canvas,
    area: Rect,
    history: &VecDeque<f64>,
    maximum: f64,
    style: theme::Style,
    offset: f64,
) {
    draw_graph_offset_options(canvas, area, history, maximum, style, false, false, offset);
}

fn draw_graph_options(
    canvas: &mut Canvas,
    area: Rect,
    history: &VecDeque<f64>,
    maximum: f64,
    style: theme::Style,
    invert: bool,
    no_zero: bool,
) {
    draw_graph_offset_options(canvas, area, history, maximum, style, invert, no_zero, 0.0);
}

#[allow(clippy::too_many_arguments)]
fn draw_graph_offset_options(
    canvas: &mut Canvas,
    area: Rect,
    history: &VecDeque<f64>,
    maximum: f64,
    style: theme::Style,
    invert: bool,
    no_zero: bool,
    offset: f64,
) {
    if area.w == 0 || area.h == 0 {
        return;
    }
    const BRAILLE: [char; 25] = [
        ' ', '⢀', '⢠', '⢰', '⢸', '⡀', '⣀', '⣠', '⣰', '⣸', '⡄', '⣄', '⣤', '⣴', '⣼', '⡆', '⣆', '⣦',
        '⣶', '⣾', '⡇', '⣇', '⣧', '⣷', '⣿',
    ];
    const BLOCK: [char; 25] = [
        ' ', '▗', '▗', '▐', '▐', '▖', '▄', '▄', '▟', '▟', '▖', '▄', '▄', '▟', '▟', '▌', '▙', '▙',
        '█', '█', '▌', '▙', '▙', '█', '█',
    ];
    const TTY: [char; 25] = [
        ' ', '░', '░', '▒', '▒', '░', '░', '▒', '▒', '█', '░', '▒', '▒', '▒', '█', '▒', '▒', '▒',
        '█', '█', '▒', '█', '█', '█', '█',
    ];
    const BRAILLE_DOWN: [char; 25] = [
        ' ', '⠈', '⠘', '⠸', '⢸', '⠁', '⠉', '⠙', '⠹', '⢹', '⠃', '⠋', '⠛', '⠻', '⢻', '⠇', '⠏', '⠟',
        '⠿', '⢿', '⡇', '⡏', '⡟', '⡿', '⣿',
    ];
    const BLOCK_DOWN: [char; 25] = [
        ' ', '▝', '▝', '▐', '▐', '▘', '▀', '▀', '▜', '▜', '▘', '▀', '▀', '▜', '▜', '▌', '▛', '▛',
        '█', '█', '▌', '▛', '▛', '█', '█',
    ];

    let tty = canvas.tty || canvas.graph_symbol == GraphSymbol::Tty;
    let values_wanted = area.w * if tty { 1 } else { 2 };
    let visible_values = history.len().min(values_wanted);
    let occupied_width = if tty {
        visible_values
    } else {
        visible_values.div_ceil(2)
    };
    let first_data_x = area.w.saturating_sub(occupied_width);
    let mut values = vec![0.0; values_wanted.saturating_sub(visible_values)];
    values.extend(history.iter().rev().take(visible_values).rev().copied());
    let normalize = |value: f64| ((value + offset) * 100.0 / maximum.max(1.0)).clamp(0.0, 100.0);

    let symbol_table = match canvas.graph_symbol {
        GraphSymbol::Braille if !canvas.tty && invert => &BRAILLE_DOWN,
        GraphSymbol::Braille if !canvas.tty => &BRAILLE,
        GraphSymbol::Block if !canvas.tty && invert => &BLOCK_DOWN,
        GraphSymbol::Block if !canvas.tty => &BLOCK,
        _ => &TTY,
    };
    let mut previous = 0.0;
    for x in 0..area.w {
        // btop pads the unused history width with spaces before creating the
        // graph. Those cells are not zero-valued samples and must not receive
        // the no_zero floor on a newly started graph.
        if x < first_data_x {
            continue;
        }
        let (left, right) = if tty {
            let current = normalize(values[x]);
            let pair = (previous, current);
            previous = current;
            pair
        } else {
            (normalize(values[x * 2]), normalize(values[x * 2 + 1]))
        };
        for row in 0..area.h {
            let high = if area.h > 1 {
                (100.0 * (area.h - row) as f64 / area.h as f64).round()
            } else {
                100.0
            };
            let low = if area.h > 1 {
                (100.0 * (area.h - row - 1) as f64 / area.h as f64).round()
            } else {
                0.0
            };
            let level = |value: f64| -> usize {
                let clamp_min = usize::from(no_zero && row + 1 == area.h);
                if value >= high {
                    4
                } else if value <= low {
                    clamp_min
                } else {
                    (((value - low) * 4.0 / (high - low) + if area.h == 1 { 0.3 } else { 0.1 })
                        .round() as usize)
                        .clamp(clamp_min, 4)
                }
            };
            let ch = symbol_table[level(left) * 5 + level(right)];
            if ch == ' ' {
                continue;
            }
            let color_value = if area.h == 1 {
                left.max(right).round().clamp(0.0, 100.0) as usize
            } else {
                (area.h - row) * 100 / area.h
            };
            canvas.put(
                area.x + x,
                area.y + if invert { area.h - row - 1 } else { row },
                ch,
                theme::with_value(style, color_value),
            );
        }
    }
}

fn meter(canvas: &mut Canvas, x: usize, y: usize, width: usize, percent: f64, style: theme::Style) {
    meter_options(canvas, x, y, width, percent, style, false, false);
}

fn meter_bold(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    percent: f64,
    style: theme::Style,
) {
    meter_options(canvas, x, y, width, percent, style, true, false);
}

fn meter_inverted(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    percent: f64,
    style: theme::Style,
) {
    meter_options(canvas, x, y, width, percent, style, false, true);
}

#[allow(clippy::too_many_arguments)]
fn meter_options(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    width: usize,
    percent: f64,
    style: theme::Style,
    bold: bool,
    invert: bool,
) {
    for index in 0..width {
        let value = (((index + 1) as f64 * 100.0 / width.max(1) as f64).round() as usize).min(100);
        let cell_style = if percent.clamp(0.0, 100.0) >= value as f64 {
            theme::with_value(style, if invert { 100 - value } else { value })
        } else {
            theme::METER_BG
        };
        if bold {
            canvas.put_bold(x + index, y, '■', cell_style);
        } else {
            canvas.put(x + index, y, '■', cell_style);
        }
    }
}

fn draw_value_unit(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    value: &str,
    unit: &str,
    value_style: theme::Style,
    unit_style: theme::Style,
) {
    canvas.text(x, y, value, value_style);
    canvas.text(x + units::display_width(value), y, unit, unit_style);
}

fn draw_value_unit_bold(
    canvas: &mut Canvas,
    x: usize,
    y: usize,
    value: &str,
    unit: &str,
    value_style: theme::Style,
    unit_style: theme::Style,
) {
    canvas.text_bold(x, y, value, value_style);
    canvas.text_bold(x + units::display_width(value), y, unit, unit_style);
}

fn ratio(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        part as f64 * 100.0 / total as f64
    }
}
fn push(history: &mut VecDeque<f64>, value: f64) {
    history.push_back(value);
    if history.len() > HISTORY {
        history.pop_front();
    }
}

fn recent_average(history: &VecDeque<f64>, count: usize) -> Option<f64> {
    let samples = history.len().min(count);
    (samples > 0).then(|| history.iter().rev().take(samples).sum::<f64>() / samples as f64)
}

#[derive(Clone, Copy)]
struct Rect {
    x: usize,
    y: usize,
    w: usize,
    h: usize,
}
impl Rect {
    fn new(x: usize, y: usize, w: usize, h: usize) -> Self {
        Self { x, y, w, h }
    }
    fn bottom(self) -> usize {
        self.y + self.h
    }
    fn contains(self, x: usize, y: usize) -> bool {
        (self.x..self.x + self.w).contains(&x) && (self.y..self.y + self.h).contains(&y)
    }
}

#[derive(Clone)]
struct Cell {
    ch: char,
    combining: String,
    continuation: bool,
    style: theme::Style,
    bold: bool,
    italic: bool,
    underline: bool,
    blink: bool,
}

struct Canvas {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    low_color: bool,
    tty: bool,
    tty_colors: bool,
    rounded: bool,
    graph_symbol: GraphSymbol,
    theme_background: bool,
    palette: theme::Palette,
}

impl Canvas {
    fn new(width: usize, height: usize) -> Self {
        Self {
            width,
            height,
            cells: vec![
                Cell {
                    ch: ' ',
                    combining: String::new(),
                    continuation: false,
                    style: theme::MAIN,
                    bold: false,
                    italic: false,
                    underline: false,
                    blink: false,
                };
                width * height
            ],
            low_color: false,
            tty: false,
            tty_colors: false,
            rounded: true,
            graph_symbol: GraphSymbol::Braille,
            theme_background: true,
            palette: theme::Palette::default(),
        }
    }
    fn put(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        if x < self.width && y < self.height {
            self.clear_wide_overlap(x, y);
            self.cells[y * self.width + x] = Cell {
                ch,
                combining: String::new(),
                continuation: false,
                style,
                bold: false,
                italic: false,
                underline: false,
                blink: false,
            };
        }
    }
    fn put_bold(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        if x < self.width && y < self.height {
            self.clear_wide_overlap(x, y);
            self.cells[y * self.width + x] = Cell {
                ch,
                combining: String::new(),
                continuation: false,
                style,
                bold: true,
                italic: false,
                underline: false,
                blink: false,
            };
        }
    }
    fn put_bold_italic(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        self.put_bold(x, y, ch, style);
        if x < self.width && y < self.height {
            self.cells[y * self.width + x].italic = true;
        }
    }
    fn put_italic(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        self.put(x, y, ch, style);
        if x < self.width && y < self.height {
            self.cells[y * self.width + x].italic = true;
        }
    }
    fn put_underline(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        self.put(x, y, ch, style);
        if x < self.width && y < self.height {
            self.cells[y * self.width + x].underline = true;
        }
    }
    fn put_blink(&mut self, x: usize, y: usize, ch: char, style: theme::Style) {
        self.put(x, y, ch, style);
        if x < self.width && y < self.height {
            self.cells[y * self.width + x].blink = true;
        }
    }
    fn text(&mut self, mut x: usize, y: usize, text: &str, style: theme::Style) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    let mut index = y * self.width + x - 1;
                    if self.cells[index].continuation && x > 1 {
                        index -= 1;
                    }
                    self.cells[index].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            self.put(x, y, ch, style);
            if width == 2 {
                let continuation = &mut self.cells[y * self.width + x + 1];
                continuation.ch = ' ';
                continuation.combining.clear();
                continuation.continuation = true;
                continuation.style = style;
                continuation.bold = false;
                continuation.italic = false;
                continuation.underline = false;
                continuation.blink = false;
            }
            x += width;
        }
    }
    fn text_bold(&mut self, mut x: usize, y: usize, text: &str, style: theme::Style) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    let mut index = y * self.width + x - 1;
                    if self.cells[index].continuation && x > 1 {
                        index -= 1;
                    }
                    self.cells[index].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            self.put_bold(x, y, ch, style);
            if width == 2 {
                let continuation = &mut self.cells[y * self.width + x + 1];
                continuation.ch = ' ';
                continuation.combining.clear();
                continuation.continuation = true;
                continuation.style = style;
                continuation.bold = true;
                continuation.italic = false;
                continuation.underline = false;
                continuation.blink = false;
            }
            x += width;
        }
    }
    fn text_bold_italic(&mut self, mut x: usize, y: usize, text: &str, style: theme::Style) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    self.cells[y * self.width + x - 1].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            self.put_bold_italic(x, y, ch, style);
            if width == 2 {
                let continuation = &mut self.cells[y * self.width + x + 1];
                continuation.ch = ' ';
                continuation.combining.clear();
                continuation.continuation = true;
                continuation.style = style;
                continuation.bold = true;
                continuation.italic = true;
                continuation.underline = false;
                continuation.blink = false;
            }
            x += width;
        }
    }
    fn text_italic(&mut self, mut x: usize, y: usize, text: &str, style: theme::Style) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    self.cells[y * self.width + x - 1].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            self.put_italic(x, y, ch, style);
            if width == 2 {
                let continuation = &mut self.cells[y * self.width + x + 1];
                continuation.ch = ' ';
                continuation.combining.clear();
                continuation.continuation = true;
                continuation.style = style;
                continuation.bold = false;
                continuation.italic = true;
                continuation.underline = false;
                continuation.blink = false;
            }
            x += width;
        }
    }
    fn clear_wide_overlap(&mut self, x: usize, y: usize) {
        let index = y * self.width + x;
        if self.cells[index].continuation && x > 0 {
            let previous = &mut self.cells[index - 1];
            previous.ch = ' ';
            previous.combining.clear();
        }
        if x + 1 < self.width && self.cells[index + 1].continuation {
            let next = &mut self.cells[index + 1];
            next.ch = ' ';
            next.combining.clear();
            next.continuation = false;
        }
    }
    fn control_title(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        hotkeys: &[usize],
        active: bool,
        line_style: theme::Style,
    ) {
        self.control_title_state(x, y, text, hotkeys, active, true, line_style);
    }
    #[allow(clippy::too_many_arguments)]
    fn control_title_state(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        hotkeys: &[usize],
        bold: bool,
        enabled: bool,
        line_style: theme::Style,
    ) {
        self.put(x, y, '┐', line_style);
        for (index, ch) in text.chars().enumerate() {
            let style = if !enabled {
                theme::LOW
            } else if hotkeys.contains(&index) {
                theme::HI
            } else {
                theme::TITLE
            };
            if bold {
                self.put_bold(x + 1 + index, y, ch, style);
            } else {
                self.put(x + 1 + index, y, ch, style);
            }
        }
        self.put(x + 1 + units::display_width(text), y, '┌', line_style);
    }
    fn control_footer(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        hotkeys: &[usize],
        active: bool,
        line_style: theme::Style,
    ) {
        self.control_footer_state(x, y, text, hotkeys, active, true, line_style);
    }
    #[allow(clippy::too_many_arguments)]
    fn control_footer_state(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        hotkeys: &[usize],
        bold: bool,
        enabled: bool,
        line_style: theme::Style,
    ) {
        self.put(x, y, '┘', line_style);
        for (index, ch) in text.chars().enumerate() {
            let style = if !enabled {
                theme::LOW
            } else if hotkeys.contains(&index) {
                theme::HI
            } else {
                theme::TITLE
            };
            if bold {
                self.put_bold(x + 1 + index, y, ch, style);
            } else {
                self.put(x + 1 + index, y, ch, style);
            }
        }
        self.put(x + 1 + units::display_width(text), y, '└', line_style);
    }
    fn decorated_label(
        &mut self,
        x: usize,
        y: usize,
        text: &str,
        left: char,
        right: char,
        line_style: theme::Style,
    ) {
        self.put(x, y, left, line_style);
        let mut chars = text.chars();
        let first = chars.next();
        let numbered = first.is_some_and(|ch| "¹²³⁴⁵⁶⁷⁸⁹⁰".contains(ch));
        if numbered {
            self.put_bold(x + 1, y, first.unwrap_or_default(), theme::HI);
            self.text_bold(x + 2, y, chars.as_str(), theme::TITLE);
        } else {
            self.text_bold(x + 1, y, text, theme::TITLE);
        }
        self.put(x + 1 + units::display_width(text), y, right, line_style);
    }
    fn title(&mut self, x: usize, y: usize, text: &str, line_style: theme::Style) {
        self.decorated_label(x, y, text, '┐', '┌', line_style);
    }
    fn footer(&mut self, x: usize, y: usize, text: &str, line_style: theme::Style) {
        self.decorated_label(x, y, text, '┘', '└', line_style);
    }
    fn title_normal(&mut self, x: usize, y: usize, text: &str, line_style: theme::Style) {
        self.put(x, y, '┐', line_style);
        self.text(x + 1, y, text, theme::TITLE);
        self.put(x + 1 + units::display_width(text), y, '┌', line_style);
    }
    fn footer_normal(&mut self, x: usize, y: usize, text: &str, line_style: theme::Style) {
        self.put(x, y, '┘', line_style);
        self.text(x + 1, y, text, theme::TITLE);
        self.put(x + 1 + units::display_width(text), y, '└', line_style);
    }
    fn text_preserve_spaces(&mut self, mut x: usize, y: usize, text: &str, style: theme::Style) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    let mut index = y * self.width + x - 1;
                    if self.cells[index].continuation && x > 1 {
                        index -= 1;
                    }
                    self.cells[index].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            if ch != ' ' {
                self.text(x, y, &ch.to_string(), style);
            }
            x += width;
        }
    }
    fn text_preserve_spaces_bold(
        &mut self,
        mut x: usize,
        y: usize,
        text: &str,
        style: theme::Style,
    ) {
        for ch in text.chars() {
            let width = units::char_width(ch);
            if width == 0 {
                if x > 0 && y < self.height {
                    self.cells[y * self.width + x - 1].combining.push(ch);
                }
                continue;
            }
            if x + width > self.width {
                break;
            }
            if ch != ' ' {
                self.put_bold(x, y, ch, style);
            }
            x += width;
        }
    }
    fn fill(&mut self, x: usize, y: usize, width: usize, ch: char, style: theme::Style) {
        for offset in 0..width {
            self.put(x + offset, y, ch, style);
        }
    }
    fn dim(&mut self) {
        for cell in &mut self.cells {
            cell.style = theme::LOW;
            cell.bold = false;
            cell.italic = false;
            cell.underline = false;
            cell.blink = false;
        }
    }
    fn panel(&mut self, area: Rect, title: &str, style: theme::Style, right: Option<String>) {
        if area.w < 2 || area.h < 2 {
            return;
        }
        let (tl, tr, bl, br, horizontal, vertical) = if self.rounded {
            ('╭', '╮', '╰', '╯', '─', '│')
        } else {
            ('┌', '┐', '└', '┘', '─', '│')
        };
        for x in area.x + 1..area.x + area.w - 1 {
            self.put(x, area.y, horizontal, style);
            self.put(x, area.y + area.h - 1, horizontal, style);
        }
        for y in area.y + 1..area.y + area.h - 1 {
            self.put(area.x, y, vertical, style);
            self.put(area.x + area.w - 1, y, vertical, style);
        }
        self.put(area.x, area.y, tl, style);
        self.put(area.x + area.w - 1, area.y, tr, style);
        self.put(area.x, area.y + area.h - 1, bl, style);
        self.put(area.x + area.w - 1, area.y + area.h - 1, br, style);
        if !title.is_empty() {
            self.title(area.x + 2, area.y, title, style);
        }
        if let Some(right) = right {
            let right = units::truncate(&right, area.w.saturating_sub(title.len() + 8));
            let x = area.x + area.w.saturating_sub(units::display_width(&right) + 3);
            self.title(x, area.y, &right, style);
        }
    }
    fn shadow(&mut self, area: Rect) {
        for y in area.y..(area.y + area.h).min(self.height) {
            for x in area.x..(area.x + area.w).min(self.width) {
                self.put(x, y, ' ', theme::MAIN);
            }
        }
    }
    fn finish(self) -> String {
        let mut output = String::with_capacity(self.width * self.height * 2);
        output.push_str(theme::RESET);
        let mut last_style = None;
        for y in 0..self.height {
            for x in 0..self.width {
                let cell = &self.cells[y * self.width + x];
                if cell.continuation {
                    continue;
                }
                let rendition = (
                    cell.style,
                    cell.bold,
                    cell.italic,
                    cell.underline,
                    cell.blink,
                );
                if Some(rendition) != last_style {
                    output.push_str(if cell.bold { "\x1b[1m" } else { "\x1b[22m" });
                    output.push_str(if cell.italic { "\x1b[3m" } else { "\x1b[23m" });
                    output.push_str(if cell.underline {
                        "\x1b[4m"
                    } else {
                        "\x1b[24m"
                    });
                    output.push_str(if cell.blink { "\x1b[5m" } else { "\x1b[25m" });
                    output.push_str(&theme::escape(
                        cell.style,
                        self.low_color,
                        self.tty_colors,
                        self.theme_background,
                        &self.palette,
                    ));
                    last_style = Some(rendition);
                }
                output.push(cell.ch);
                output.push_str(&cell.combining);
            }
            if y + 1 < self.height {
                output.push_str("\x1b[0m\r\n");
                last_style = None;
            }
        }
        output.push_str(theme::RESET);
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::collect::DiskSample;

    fn app() -> AppState {
        let mut app = AppState::new(Config::default());
        app.last_size = Some(Size {
            cols: 120,
            rows: 40,
        });
        app
    }

    fn canvas_text(canvas: &Canvas) -> String {
        canvas
            .cells
            .chunks(canvas.width)
            .map(|row| row.iter().map(|cell| cell.ch).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn canvas_row(canvas: &Canvas, y: usize) -> String {
        canvas.cells[y * canvas.width..(y + 1) * canvas.width]
            .iter()
            .map(|cell| cell.ch)
            .collect()
    }

    fn frame_fingerprint(frame: &str) -> u64 {
        frame.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        })
    }

    #[test]
    fn deterministic_full_frames_cover_reference_terminal_layouts() {
        let cases = [
            (Size { cols: 80, rows: 24 }, "cpu"),
            (
                Size {
                    cols: 120,
                    rows: 40,
                },
                "cpu mem net proc",
            ),
            (
                Size {
                    cols: 160,
                    rows: 50,
                },
                "mem net proc",
            ),
        ];
        let actual = cases.map(|(size, boxes)| {
            let mut app = app();
            app.config.clock_format.clear();
            app.config.set_value("shown_boxes", boxes);
            let mut renderer = Renderer::new();
            let frame = renderer.render(size, &mut app);
            frame_fingerprint(&frame)
        });
        assert_eq!(
            actual,
            [
                2_770_106_502_089_868_106,
                18_097_391_836_841_027_499,
                14_449_041_907_813_186_248,
            ]
        );
    }

    #[test]
    fn options_arrows_change_boolean_and_choice_values() {
        let mut app = app();
        app.handle_key(Key::Char('o'));
        app.handle_key(Key::Down);
        assert_eq!(app.current_option(), Some("theme_background"));
        let old = app.config.theme_background;
        app.handle_key(Key::Right);
        assert_eq!(app.config.theme_background, !old);

        app.handle_key(Key::Char('6'));
        app.handle_key(Key::Down);
        app.handle_key(Key::Down);
        assert_eq!(app.current_option(), Some("proc_sorting"));
        app.handle_key(Key::Right);
        assert_eq!(app.config.process_sort, ProcessSort::Pid);
    }

    #[test]
    fn escape_toggles_the_main_menu_without_quitting() {
        let mut app = app();

        assert!(!app.handle_key(Key::Escape));
        assert_eq!(app.overlay, Overlay::Main { selected: 0 });
        assert!(!app.handle_key(Key::Escape));
        assert_eq!(app.overlay, Overlay::None);
        assert!(app.handle_key(Key::Char('q')));
    }

    #[test]
    fn debug_mode_draws_per_box_collection_and_render_timings() {
        let mut app = app();
        app.set_debug(true);
        app.sample.collection_times_us = [11, 22, 33, 44, 55, 165];
        let mut renderer = Renderer::new();
        let frame = renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );

        assert!(frame.contains("box       collect         draw"));
        assert!(frame.contains("cpu             11"));
        assert!(frame.contains("gpu             55"));
        assert!(frame.contains("total          165"));
    }

    #[test]
    fn process_arrows_work_without_vim_and_vim_kill_uses_uppercase_k() {
        let mut app = app();
        app.handle_key(Key::Down);
        assert!(app.process_selected);
        assert_eq!(app.selected_process, 0);
        app.handle_key(Key::Up);
        assert!(!app.process_selected);

        app.config.vim_keys = true;
        app.visible_pids.push(123);
        app.handle_key(Key::Char('j'));
        assert!(app.process_selected);
        app.handle_key(Key::Char('K'));
        assert_eq!(
            app.overlay,
            Overlay::Signal {
                pid: 123,
                signal: 9
            }
        );
    }

    #[test]
    fn process_paging_wheel_and_detail_follow_use_rendered_rows() {
        let mut app = app();
        app.visible_pids = (1..=40).collect();
        app.process_area = Some(Rect::new(50, 10, 70, 20));
        app.process_scrollbar = Some(ProcessScrollbar {
            x: 118,
            up_y: 11,
            down_y: 28,
            track_top: 12,
            track_bottom: 28,
            thumb_y: 12,
            visible: 10,
            total: 40,
        });
        app.process_selected = true;
        app.selected_process = 5;

        app.handle_key(Key::PageUp);
        assert!(!app.process_selected);
        app.handle_key(Key::PageDown);
        assert!(app.process_selected);
        assert_eq!(app.selected_process, 0);

        app.process_offset = 5;
        app.selected_process = 7;
        app.handle_key(Key::PageDown);
        assert_eq!(app.process_offset, 15);
        assert_eq!(app.selected_process, 17);

        app.handle_key(Key::Mouse {
            button: 65,
            x: 60,
            y: 15,
            pressed: true,
        });
        assert_eq!(app.process_offset, 18);
        assert_eq!(app.selected_process, 20);

        app.handle_key(Key::Enter);
        assert_eq!(app.detailed_pid, Some(21));
        assert_eq!(app.followed_pid, Some(21));
        assert!(!app.process_selected);
        app.handle_key(Key::Down);
        assert!(app.process_selected);
        assert_eq!(app.selected_process, 21);
        assert_eq!(app.followed_pid, None);
    }

    #[test]
    fn unselected_process_wheel_offset_survives_each_redraw() {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        app.sample.processes = (1..=100)
            .map(|pid| ProcessSample {
                pid,
                parent: 0,
                name: format!("process-{pid}"),
                ..ProcessSample::default()
            })
            .collect();
        app.sample.process_count = app.sample.processes.len();
        let size = Size {
            cols: 100,
            rows: 30,
        };
        let mut renderer = Renderer::new();
        renderer.render(size, &mut app);
        assert!(!app.process_selected);

        for expected in [3, 6, 9, 12] {
            app.handle_key(Key::Mouse {
                button: 65,
                x: 50,
                y: 15,
                pressed: true,
            });
            assert_eq!(app.process_offset, expected);
            renderer.render(size, &mut app);
            assert_eq!(app.process_offset, expected);
            assert!(!app.process_selected);
        }
    }

    #[test]
    fn overlays_dim_the_existing_screen_to_inactive_fg() {
        let mut canvas = Canvas::new(3, 1);
        canvas.put(0, 0, 'a', theme::CPU);
        canvas.put(1, 0, 'b', theme::TITLE);
        canvas.put(2, 0, 'c', theme::PROC_BOX);

        canvas.dim();

        assert!(canvas.cells.iter().all(|cell| cell.style == theme::LOW));
    }

    #[test]
    fn options_enter_edits_integer_values() {
        let mut app = app();
        app.handle_key(Key::Char('o'));
        for _ in 0..9 {
            app.handle_key(Key::Down);
        }
        assert_eq!(app.current_option(), Some("update_ms"));
        app.handle_key(Key::Enter);
        assert!(app.options_editing);
        app.handle_key(Key::Delete);
        for ch in "3100".chars() {
            app.handle_key(Key::Char(ch));
        }
        app.handle_key(Key::Enter);
        assert!(!app.options_editing);
        assert_eq!(app.config.update_ms, 3100);
    }

    #[test]
    fn source_centering_biases_odd_padding_and_even_boxes_left() {
        assert_eq!(
            center_text("catppuccin-mocha", 25),
            "     catppuccin-mocha    "
        );
        assert_eq!(source_center_x(180, 78), 50);
        assert_eq!(source_center_x(180, 19), 80);
        assert_eq!(source_center_x(180, 12), 83);

        let mut canvas = Canvas::new(180, 6);
        draw_banner(&mut canvas, 0);
        let version = canvas.cells[5 * 180..6 * 180]
            .iter()
            .position(|cell| cell.ch == 'v')
            .unwrap();
        assert_eq!(version, source_center_x(180, 49) + 42);
        assert!(canvas.cells[5 * 180 + version].bold);
        assert!(canvas.cells[5 * 180 + version].italic);
    }

    #[test]
    fn full_height_options_geometry_and_integer_edit_controls_match_btop() {
        let mut app = app();
        app.last_size = Some(Size {
            cols: 180,
            rows: 60,
        });
        app.overlay = Overlay::Options;
        app.options_selected = 9;
        let area = app.options_area().unwrap();
        assert_eq!((area.x, area.y, area.w, area.h), (50, 11, 78, 46));

        let mut canvas = Canvas::new(180, 60);
        draw_options(&mut canvas, &app, 0, 0, 9);
        let value_y = area.y + 3 + 9 * 2 + 1;
        assert_eq!(canvas.cells[value_y * 180 + area.x + 2].ch, '←');
        assert_eq!(canvas.cells[value_y * 180 + area.x + 26].ch, '↵');
        assert_eq!(canvas.cells[value_y * 180 + area.x + 28].ch, '→');
        assert!(canvas_text(&canvas).contains("Save config on exit"));
        assert!(!canvas_text(&canvas).contains("page 1/"));

        app.handle_options_mouse(0, area.x + 10, value_y, true);
        assert!(app.options_editing, "the numeric value is mouse-editable");
        let mut editing_canvas = Canvas::new(180, 60);
        draw_options(&mut editing_canvas, &app, 0, 0, 9);
        assert_eq!(
            editing_canvas
                .cells
                .iter()
                .filter(|cell| cell.underline)
                .count(),
            1
        );
    }

    #[test]
    fn text_editor_cursor_attributes_reach_the_terminal_output() {
        let mut canvas = Canvas::new(2, 1);
        canvas.put_underline(0, 0, ' ', theme::MAIN);
        canvas.put_blink(1, 0, '█', theme::MAIN);
        let output = canvas.finish();
        assert!(output.contains("\x1b[4m"));
        assert!(output.contains("\x1b[5m"));
    }

    #[test]
    fn help_uses_the_exact_reference_height_without_a_trailing_blank_row() {
        let mut canvas = Canvas::new(180, 60);
        draw_help(&mut canvas, 0);
        assert_eq!(canvas.cells[8 * 180 + 50].ch, '╭');
        assert_eq!(canvas.cells[56 * 180 + 50].ch, '╰');
        assert!(canvas_row(&canvas, 55).contains("https://github.com/aristocratos/btop"));
    }

    #[test]
    fn memory_network_height_matches_btop_percentages() {
        assert_eq!(memory_panel_height(40, 9, 12, true, true, true, true), 10);
        assert_eq!(memory_panel_height(40, 13, 0, true, true, true, false), 15);
        assert_eq!(memory_panel_height(40, 9, 12, true, true, false, true), 19);
        assert_eq!(memory_panel_height(40, 9, 12, true, false, true, true), 0);
    }

    #[test]
    fn multi_gpu_heights_use_calc_sizes_integer_order() {
        let gpu = GpuSample::default();
        assert_eq!(gpu_panel_height(&gpu, 0, true, 20, 2, 20, 2), 8);
        assert_eq!(gpu_panel_height(&gpu, 9, true, 10, 1, 19, 1), 10);

        let mut app = app();
        app.config
            .set_value("shown_boxes", "cpu gpu0 gpu1 gpu2 mem");
        app.sample.gpus = vec![gpu.clone(), gpu.clone(), gpu];
        app.gpu_histories = (0..3).map(|_| GpuHistory::default()).collect();
        let mut renderer = Renderer::new();
        renderer.render(
            Size {
                cols: 100,
                rows: 100,
            },
            &mut app,
        );
        assert_eq!(app.cpu_area.map(|area| area.h), Some(13));
    }

    #[test]
    fn minimum_size_matches_reference_box_combinations() {
        let mut config = Config::default();
        assert_eq!(minimum_size(&config, &[]), Size { cols: 80, rows: 24 });

        config.set_value("shown_boxes", "cpu mem net");
        assert_eq!(minimum_size(&config, &[]), Size { cols: 60, rows: 24 });

        config.set_value("shown_boxes", "gpu0");
        let mut gpu = GpuSample::default();
        gpu.support.utilization = true;
        gpu.support.power = true;
        gpu.support.memory_total = true;
        gpu.support.memory_used = true;
        assert_eq!(minimum_size(&config, &[gpu]), Size { cols: 41, rows: 9 });
    }

    #[test]
    fn gpu_decoder_value_stays_inside_the_statistics_box() {
        let mut app = app();
        let mut gpu = GpuSample::default();
        gpu.support.utilization = true;
        gpu.support.power = true;
        gpu.support.temperature = true;
        gpu.support.encoder = true;
        gpu.support.decoder = true;
        gpu.utilization = 29;
        gpu.temperature_c = 55;
        gpu.temperature_max_c = 100;
        gpu.power_mw = 38_000;
        gpu.power_limit_mw = 100_000;
        gpu.encoder_utilization = 10;
        gpu.decoder_utilization = 12;
        app.sample.gpus.push(gpu);
        app.gpu_histories.push(GpuHistory::default());
        let mut canvas = Canvas::new(100, 12);

        draw_gpu(&mut canvas, Rect::new(0, 0, 100, 12), &app, 0);

        let box_width = 50;
        let box_x = 100 - box_width - 1;
        let box_height = 5;
        let box_y = (12usize.saturating_sub(2 + box_height)).div_ceil(2) + 1;
        let decoder_row = box_y + 3;
        assert_eq!(
            canvas.cells[decoder_row * 100 + box_x + box_width - 1].ch,
            '│'
        );
        assert_eq!(
            canvas.cells[decoder_row * 100 + box_x + box_width - 2].ch,
            '%'
        );
        assert_eq!(canvas.cells[5 * 100 + 82].style, theme::Style::Cpu(29));
        assert!(canvas.cells[5 * 100 + 50].bold);
        assert!(canvas.cells[5 * 100 + 54].bold);
        assert!(canvas.cells[5 * 100 + 82].bold);
        assert_eq!(canvas.cells[5 * 100 + 84].style, theme::MAIN);
        assert_eq!(canvas.cells[5 * 100 + 94].style, theme::Style::Temp(55));
        assert!(canvas.cells[5 * 100 + 94].bold);
        assert_eq!(canvas.cells[5 * 100 + 96].style, theme::MAIN);
        assert!(canvas.cells[5 * 100 + 96].bold);
        assert_eq!(canvas.cells[6 * 100 + 94].style, theme::Style::Cached(38));
        assert!(canvas.cells[6 * 100 + 94].bold);
        assert!(canvas.cells[6 * 100 + 50].bold);
        assert!(canvas.cells[6 * 100 + 54].bold);
        assert_eq!(canvas.cells[6 * 100 + 97].style, theme::MAIN);
        assert!(canvas.cells[6 * 100 + 97].bold);
        assert_eq!(canvas.cells[7 * 100 + 71].style, theme::Style::Cpu(10));
        assert!(canvas.cells[7 * 100 + 50].bold);
        assert!(canvas.cells[7 * 100 + 54].bold);
        assert!(canvas.cells[7 * 100 + 71].bold);
        assert!(canvas.cells[7 * 100 + 75].bold);
        assert_eq!(canvas.cells[7 * 100 + 73].style, theme::MAIN);
        assert!(canvas.cells[7 * 100 + 73].bold);
        assert_eq!(canvas.cells[7 * 100 + 95].style, theme::Style::Cpu(12));
        assert!(canvas.cells[7 * 100 + 95].bold);
        assert_eq!(canvas.cells[7 * 100 + 97].style, theme::MAIN);
        assert!(canvas.cells[7 * 100 + 97].bold);
    }

    #[test]
    fn non_tree_process_columns_and_value_formats_match_btop() {
        let mut app = app();
        app.config.process_tree = false;
        app.config.process_mem_bytes = false;
        app.config.process_sort = ProcessSort::Pid;
        app.sample.memory.total = 1_000;
        app.sample.processes = vec![ProcessSample {
            pid: 42,
            name: "program".into(),
            command: "/usr/bin/program --flag".into(),
            user: "tester".into(),
            threads: 12_345,
            memory: 123,
            cpu: 12.34,
            ..ProcessSample::default()
        }];
        let mut canvas = Canvas::new(100, 15);

        draw_processes(&mut canvas, Rect::new(0, 0, 100, 15), &mut app);

        let header = canvas_row(&canvas, 1);
        let process = canvas_row(&canvas, 2);
        assert!(header.contains("Pid: Program:         Command:"));
        assert!(process.contains("      42 program          /usr/bin/program --flag"));
        assert!(process.contains(" 12K tester"));
        assert!(process.contains(" 12%"));
        assert!(process.contains("12.34"));
    }

    #[test]
    fn gpu_vram_percentages_use_the_normal_foreground() {
        let mut app = app();
        let mut gpu = GpuSample {
            memory_total: 16 * 1024 * 1024,
            memory_used: 8 * 1024 * 1024,
            memory_utilization: 7,
            ..GpuSample::default()
        };
        gpu.support.memory_total = true;
        gpu.support.memory_used = true;
        gpu.support.memory_utilization = true;
        app.sample.gpus.push(gpu);
        app.gpu_histories.push(GpuHistory::default());
        let mut canvas = Canvas::new(100, 12);

        draw_gpu(&mut canvas, Rect::new(0, 0, 100, 12), &app, 0);

        // The graphs retain their Used/Free gradients, but btop resets the
        // rendition before printing both numeric percentages.
        assert!(canvas.cells[5 * 100 + 51].bold);
        assert_eq!(canvas.cells[5 * 100 + 77].ch, '5');
        assert_eq!(canvas.cells[5 * 100 + 77].style, theme::MAIN);
        assert!(!canvas.cells[5 * 100 + 77].bold);
        assert_eq!(canvas.cells[7 * 100 + 52].ch, '7');
        assert_eq!(canvas.cells[7 * 100 + 52].style, theme::MAIN);
    }

    #[test]
    fn configured_panel_positions_move_the_real_hitboxes() {
        let mut app = app();
        app.config.set_value("cpu_bottom", "true");
        app.config.set_value("proc_left", "true");
        app.config.set_value("mem_below_net", "true");
        let mut renderer = Renderer::new();

        renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );

        let cpu = app.cpu_area.unwrap();
        let memory = app.memory_area.unwrap();
        let network = app.network_area.unwrap();
        let process = app.process_area.unwrap();
        assert_eq!(cpu.y + cpu.h, 40);
        assert_eq!(process.x, 0);
        assert!(memory.x > process.x);
        assert!(network.y < memory.y);
        assert!(app.cpu_control_hitboxes.iter().all(|hitbox| hitbox.y == 39));
    }

    #[test]
    fn swapped_network_direction_changes_graph_and_stats_order() {
        let mut app = app();
        app.config.set_value("swap_upload_download", "true");
        app.sample.network.connected = true;
        app.sample.network.selected = "eth0".into();
        app.download_history.push_back(10.0);
        app.upload_history.push_back(20.0);
        let mut canvas = Canvas::new(70, 20);

        draw_network(&mut canvas, Rect::new(0, 0, 70, 20), &mut app);

        let stats_y = (20usize.saturating_sub(2) / 2) - 9 / 2 + 1;
        assert!(canvas_row(&canvas, stats_y).contains("upload"));
        assert!(canvas_row(&canvas, stats_y + 8).contains("download"));
        assert!(matches!(
            canvas.cells[canvas.width + 41].style,
            theme::Style::Upload(_)
        ));
    }

    #[test]
    fn network_graphs_grow_outward_from_the_middle_seam() {
        let mut app = app();
        app.config.net_auto = false;
        app.config.set_value("net_download", "1");
        app.config.set_value("net_upload", "1");
        app.sample.network.connected = true;
        app.sample.network.selected = "eth0".into();
        app.sample.network.interfaces = vec!["eth0".into()];
        app.upload_history.push_back(13_107.0);
        let mut canvas = Canvas::new(70, 20);

        draw_network(&mut canvas, Rect::new(0, 0, 70, 20), &mut app);

        let last_graph_x = 41;
        let middle_y = 10;
        assert_ne!(canvas.cells[middle_y * 70 + last_graph_x].ch, ' ');
        assert_eq!(canvas.cells[18 * 70 + last_graph_x].ch, ' ');
    }

    #[test]
    fn zero_network_usage_keeps_both_middle_floors() {
        let mut app = app();
        app.config.net_auto = false;
        app.sample.network.connected = true;
        app.sample.network.selected = "eth0".into();
        app.sample.network.interfaces = vec!["eth0".into()];
        app.download_history = VecDeque::from(vec![0.0; 140]);
        app.upload_history = VecDeque::from(vec![0.0; 140]);
        let mut canvas = Canvas::new(70, 20);

        draw_network(&mut canvas, Rect::new(0, 0, 70, 20), &mut app);

        for y in [9, 10] {
            assert!(
                canvas.cells[y * 70 + 1..y * 70 + 24]
                    .iter()
                    .all(|cell| cell.ch != ' ')
            );
        }
    }

    #[test]
    fn one_row_graph_color_tracks_the_sample_value() {
        let mut canvas = Canvas::new(1, 1);
        draw_graph(
            &mut canvas,
            Rect::new(0, 0, 1, 1),
            &VecDeque::from([25.0, 25.0]),
            100.0,
            theme::Style::Available(100),
        );
        assert_eq!(canvas.cells[0].style, theme::Style::Available(25));
    }

    #[test]
    fn graph_offsets_and_meter_thresholds_match_btop() {
        let mut graph = Canvas::new(1, 1);
        draw_graph_offset(
            &mut graph,
            Rect::new(0, 0, 1, 1),
            &VecDeque::from([73.0, 73.0]),
            100.0,
            theme::Style::Temp(100),
            -23.0,
        );
        assert_eq!(graph.cells[0].style, theme::Style::Temp(50));

        let mut meter_canvas = Canvas::new(10, 1);
        meter(&mut meter_canvas, 0, 0, 10, 49.0, theme::Style::Cpu(100));
        assert_eq!(
            meter_canvas
                .cells
                .iter()
                .filter(|cell| cell.style != theme::METER_BG)
                .count(),
            4
        );

        meter_inverted(&mut meter_canvas, 0, 0, 10, 100.0, theme::Style::Cpu(100));
        assert_eq!(meter_canvas.cells[0].style, theme::Style::Cpu(90));
        assert_eq!(meter_canvas.cells[9].style, theme::Style::Cpu(0));
    }

    #[test]
    fn graph_floor_only_covers_collected_history() {
        let mut canvas = Canvas::new(10, 2);
        draw_graph_options(
            &mut canvas,
            Rect::new(0, 0, 10, 2),
            &VecDeque::from([0.0]),
            100.0,
            theme::CPU,
            false,
            true,
        );

        assert!(canvas.cells[10..19].iter().all(|cell| cell.ch == ' '));
        assert_ne!(canvas.cells[19].ch, ' ');
    }

    #[test]
    fn inverted_graph_reverses_glyphs_but_not_the_source_gradient() {
        let mut canvas = Canvas::new(1, 4);
        draw_graph_options(
            &mut canvas,
            Rect::new(0, 0, 1, 4),
            &VecDeque::from([100.0, 100.0]),
            100.0,
            theme::CPU,
            true,
            true,
        );

        assert_eq!(canvas.cells[0].style, theme::Style::Cpu(25));
        assert_eq!(canvas.cells[3].style, theme::Style::Cpu(100));
    }

    #[test]
    fn zero_cpu_usage_draws_the_btop_graph_floor() {
        let mut app = app();
        app.cpu_history = VecDeque::from(vec![0.0; 240]);
        let mut canvas = Canvas::new(120, 20);

        draw_cpu(&mut canvas, Rect::new(0, 0, 120, 20), &mut app);

        // btop constructs both CPU graphs with no_zero=true. The upper graph
        // therefore has a floor on its bottom row and the inverted lower graph
        // has one on its top row, even while every sample is zero.
        for y in [9, 10] {
            assert!(
                canvas.cells[y * 120 + 1..y * 120 + 24]
                    .iter()
                    .all(|cell| cell.ch != ' ')
            );
        }
    }

    #[test]
    fn cpu_summary_keeps_btop_row_boldness() {
        let mut app = app();
        app.sample.cpu.cores = vec![1.0];
        app.sample.cpu.core_temperatures = vec![Some(50.0)];
        app.sample.cpu.temperature = Some(50.0);
        app.sample.cpu.temperature_max = 100.0;
        app.sample.cpu.watts = Some(10.0);
        app.core_histories = vec![VecDeque::from([1.0])];
        app.core_temperature_histories = vec![VecDeque::from([50.0])];
        app.temp_history = VecDeque::from([50.0]);
        let mut canvas = Canvas::new(100, 12);

        draw_cpu(&mut canvas, Rect::new(0, 0, 100, 12), &mut app);

        let row = 4 * 100;
        for x in [67, 71, 75, 79, 81, 86, 90, 92, 97] {
            assert!(
                canvas.cells[row + x].bold,
                "column {x} lost the row's bold rendition"
            );
        }
    }

    #[test]
    fn odd_core_cpu_geometry_uses_source_column_count() {
        let mut app = app();
        app.sample.cpu.cores = vec![0.0; 7];
        app.sample.cpu.core_temperatures = vec![None; 7];
        app.core_histories = (0..7).map(|_| VecDeque::new()).collect();
        app.core_temperature_histories = (0..7).map(|_| VecDeque::new()).collect();
        let mut canvas = Canvas::new(100, 9);

        draw_cpu(&mut canvas, Rect::new(0, 0, 100, 9), &mut app);

        assert_eq!(canvas.cells[100 + 58].ch, '╭');
        assert_eq!(canvas.cells[7 * 100 + 58].ch, '╰');
    }

    #[test]
    fn selected_process_keeps_the_cpu_graph_background_glyphs() {
        let mut app = app();
        app.config.process_tree = false;
        app.process_selected = true;
        app.sample.processes = vec![ProcessSample {
            pid: 42,
            name: "zero".into(),
            ..ProcessSample::default()
        }];
        app.process_cpu_histories.insert(42, VecDeque::from([0.0]));
        let mut canvas = Canvas::new(80, 15);

        draw_processes(&mut canvas, Rect::new(0, 0, 80, 15), &mut app);

        for x in 67..72 {
            let cell = &canvas.cells[2 * 80 + x];
            assert_eq!(cell.ch, '⣀');
            assert_eq!(cell.style, theme::SELECTED);
            assert!(cell.bold);
        }
    }

    #[test]
    fn zero_gpu_usage_draws_the_btop_graph_floor() {
        let mut app = app();
        let mut gpu = GpuSample::default();
        gpu.support.utilization = true;
        app.sample.gpus.push(gpu);
        app.gpu_histories.push(GpuHistory {
            utilization: VecDeque::from(vec![0.0; 200]),
            ..GpuHistory::default()
        });
        let mut canvas = Canvas::new(100, 12);

        draw_gpu(&mut canvas, Rect::new(0, 0, 100, 12), &app, 0);

        for y in [5, 6] {
            assert!(
                canvas.cells[y * 100 + 1..y * 100 + 24]
                    .iter()
                    .all(|cell| cell.ch != ' ')
            );
        }
    }

    #[test]
    fn manual_network_limits_are_configured_in_mebibits() {
        let mut config = Config::default();
        config.set_value("net_download", "80");
        assert_eq!(
            configured_network_max(&config, "net_download"),
            10.0 * 1024.0 * 1024.0
        );
    }

    #[test]
    fn process_follow_is_a_real_footer_control_and_tracks_the_pid() {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        app.config.process_sort = ProcessSort::Pid;
        app.sample.processes = vec![
            ProcessSample {
                pid: 10,
                name: "ten".into(),
                ..ProcessSample::default()
            },
            ProcessSample {
                pid: 20,
                name: "twenty".into(),
                ..ProcessSample::default()
            },
        ];
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 100,
            rows: 30,
        };
        renderer.render(size, &mut app);
        app.handle_key(Key::Down);
        app.handle_key(Key::Char('F'));
        assert_eq!(app.followed_pid, Some(20));

        renderer.render(size, &mut app);

        let row = app
            .process_hitboxes
            .iter()
            .find(|hitbox| hitbox.pid == 20)
            .unwrap()
            .y;
        assert_eq!(app.selected_process, 0);
        assert_eq!(
            app.process_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == ProcessControlAction::Follow)
                .unwrap()
                .y,
            29
        );
        let mut canvas = Canvas::new(100, 30);
        draw_processes(&mut canvas, Rect::new(0, 0, 100, 30), &mut app);
        assert_eq!(canvas.cells[row * canvas.width + 1].style, theme::FOLLOWED);

        let follow = app
            .process_control_hitboxes
            .iter()
            .find(|hitbox| hitbox.action == ProcessControlAction::Follow)
            .copied()
            .expect("Follow control is visible");
        let mouse = |app: &mut AppState, button| {
            app.handle_key(Key::Mouse {
                button,
                x: follow.start as u16,
                y: follow.y as u16,
                pressed: true,
            });
        };

        mouse(&mut app, 0);
        assert_eq!(app.followed_pid, None, "a click stops following");
        mouse(&mut app, 32);
        assert_eq!(
            app.followed_pid, None,
            "pointer motion is not a second click"
        );
        mouse(&mut app, 0);
        assert_eq!(app.followed_pid, Some(20), "a click starts following");
        mouse(&mut app, 0);
        assert_eq!(app.followed_pid, None, "a second click stops following");
    }

    #[test]
    fn process_info_footer_click_toggles_details_like_return() {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        app.sample.processes = vec![ProcessSample {
            pid: 42,
            name: "answer".into(),
            ..ProcessSample::default()
        }];
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 100,
            rows: 30,
        };
        renderer.render(size, &mut app);
        app.handle_key(Key::Down);
        renderer.render(size, &mut app);

        let click_info_return_glyph = |app: &mut AppState| {
            let info = app
                .process_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == ProcessControlAction::Info)
                .copied()
                .expect("Info control is visible");
            app.handle_key(Key::Mouse {
                button: 0,
                x: info.end.saturating_sub(1) as u16,
                y: info.y as u16,
                pressed: true,
            });
        };

        click_info_return_glyph(&mut app);
        assert_eq!(app.detailed_pid, Some(42));
        renderer.render(size, &mut app);
        click_info_return_glyph(&mut app);
        assert_eq!(app.detailed_pid, None);
    }

    #[test]
    fn paused_processes_become_dead_and_obey_keep_usage() {
        let mut app = app();
        app.config.pause_processes = true;
        app.sample.processes = vec![ProcessSample {
            pid: 42,
            state: 'S',
            cpu: 12.5,
            memory: 4096,
            ..ProcessSample::default()
        }];
        app.update(Sample::default());
        assert_eq!(app.sample.processes[0].state, 'X');
        assert_eq!(app.sample.processes[0].cpu, 0.0);
        assert_eq!(app.sample.processes[0].memory, 0);

        app.config.set_value("keep_dead_proc_usage", "true");
        app.sample.processes[0].state = 'S';
        app.sample.processes[0].cpu = 12.5;
        app.sample.processes[0].memory = 4096;
        app.update(Sample::default());
        assert_eq!(app.sample.processes[0].state, 'X');
        assert_eq!(app.sample.processes[0].cpu, 12.5);
        assert_eq!(app.sample.processes[0].memory, 4096);
    }

    #[test]
    fn dead_process_keeps_its_frozen_detail_entry() {
        let mut app = app();
        app.detailed_pid = Some(42);
        app.update(Sample {
            processes: vec![ProcessSample {
                pid: 42,
                name: "short-lived".into(),
                state: 'R',
                elapsed_seconds: 17,
                cpu: 8.5,
                ..ProcessSample::default()
            }],
            ..Sample::default()
        });
        app.update(Sample::default());

        let detail = app.detailed_process.as_ref().unwrap();
        assert_eq!(app.detailed_pid, Some(42));
        assert_eq!(detail.state, 'X');
        assert_eq!(detail.elapsed_seconds, 17);
        assert_eq!(detail.cpu, 8.5);
    }

    #[test]
    fn signal_dialog_buttons_and_signal_rows_are_mouse_targets() {
        let mut app = app();
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 100,
            rows: 30,
        };
        app.overlay = Overlay::Signal {
            pid: u32::MAX,
            signal: 15,
        };
        renderer.render(size, &mut app);
        let cancel = app
            .signal_confirm_hitboxes
            .iter()
            .find(|(confirm, _)| !confirm)
            .unwrap()
            .1;
        app.handle_key(Key::Mouse {
            button: 0,
            x: cancel.x as u16,
            y: cancel.y as u16,
            pressed: true,
        });
        assert_eq!(app.overlay, Overlay::None);

        app.overlay = Overlay::SignalChoose {
            pid: u32::MAX,
            selected: 15,
        };
        renderer.render(size, &mut app);
        let signal = app
            .signal_choice_hitboxes
            .iter()
            .find(|(signal, _)| *signal == 9)
            .unwrap()
            .1;
        app.handle_key(Key::Mouse {
            button: 0,
            x: signal.x as u16,
            y: signal.y as u16,
            pressed: true,
        });
        assert_eq!(
            app.overlay,
            Overlay::SignalChoose {
                pid: u32::MAX,
                selected: 9
            }
        );
    }

    #[test]
    fn mouse_motion_does_not_activate_overlay_click_targets() {
        let mut app = app();
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 100,
            rows: 30,
        };

        app.overlay = Overlay::Main { selected: 0 };
        renderer.render(size, &mut app);
        let menu = app
            .main_menu_hitboxes
            .iter()
            .find(|hitbox| hitbox.item == 0)
            .copied()
            .unwrap();
        app.handle_key(Key::Mouse {
            button: 32,
            x: menu.x as u16,
            y: menu.y as u16,
            pressed: true,
        });
        assert_eq!(app.overlay, Overlay::Main { selected: 0 });

        app.overlay = Overlay::Signal {
            pid: u32::MAX,
            signal: 15,
        };
        renderer.render(size, &mut app);
        let cancel = app
            .signal_confirm_hitboxes
            .iter()
            .find(|(confirm, _)| !confirm)
            .unwrap()
            .1;
        app.handle_key(Key::Mouse {
            button: 32,
            x: cancel.x as u16,
            y: cancel.y as u16,
            pressed: true,
        });
        assert_eq!(
            app.overlay,
            Overlay::Signal {
                pid: u32::MAX,
                signal: 15
            }
        );

        app.overlay = Overlay::Options;
        renderer.render(size, &mut app);
        app.handle_key(Key::Mouse {
            button: 32,
            x: 0,
            y: 0,
            pressed: true,
        });
        assert_eq!(app.overlay, Overlay::Options);

        app.overlay = Overlay::OperationError {
            operation: Operation::Signal,
            errno: 1,
        };
        app.handle_key(Key::Mouse {
            button: 32,
            x: 0,
            y: 0,
            pressed: true,
        });
        assert_eq!(
            app.overlay,
            Overlay::OperationError {
                operation: Operation::Signal,
                errno: 1
            }
        );
    }

    #[test]
    fn disk_io_toggle_draws_live_read_and_write_graphs() {
        let mut app = app();
        app.config.set_value("io_mode", "true");
        app.sample.memory.disks.push(DiskSample {
            mount: "/".into(),
            total: 2 * 1024 * 1024 * 1024,
            used: 1024 * 1024 * 1024,
            free: 1024 * 1024 * 1024,
            io_supported: true,
            read_per_second: 1024 * 1024,
            write_per_second: 2 * 1024 * 1024,
            io_activity: 25.0,
        });
        app.disk_histories.insert(
            "/".into(),
            DiskHistory {
                read: VecDeque::from([0.0, 1024.0 * 1024.0]),
                write: VecDeque::from([0.0, 2.0 * 1024.0 * 1024.0]),
                activity: VecDeque::from([0.0, 25.0]),
            },
        );
        let mut canvas = Canvas::new(80, 30);

        draw_memory(&mut canvas, Rect::new(0, 0, 80, 30), &mut app);

        let output = canvas_text(&canvas);
        assert!(output.contains("▲1.0M"));
        assert!(output.contains("▼2.0M"));
        assert!(output.contains("IO%"));
    }

    #[test]
    fn inline_swap_has_the_same_section_and_percentage_basis_as_btop() {
        let mut app = app();
        app.config.show_disks = false;
        app.config.set_value("show_swap", "true");
        app.config.set_value("swap_disk", "false");
        app.sample.memory.total = 64 * 1024 * 1024 * 1024;
        app.sample.memory.used = 16 * 1024 * 1024 * 1024;
        app.sample.memory.available = 48 * 1024 * 1024 * 1024;
        app.sample.memory.free = 32 * 1024 * 1024 * 1024;
        app.sample.memory.cached = 16 * 1024 * 1024 * 1024;
        app.sample.memory.swap_total = 8 * 1024 * 1024 * 1024;
        app.sample.memory.swap_used = 2 * 1024 * 1024 * 1024;
        app.swap_used_history = VecDeque::from([25.0]);
        app.swap_free_history = VecDeque::from([75.0]);
        let mut canvas = Canvas::new(50, 30);

        draw_memory(&mut canvas, Rect::new(0, 0, 50, 30), &mut app);

        let output = canvas_text(&canvas);
        assert!(output.contains("Swap:"));
        assert!(output.contains("8.00 GiB"));
        let swap_row = (0..30)
            .find(|&y| canvas_row(&canvas, y).contains("Swap:"))
            .expect("swap header");
        assert!(canvas_row(&canvas, swap_row + 1).contains("Used:"));
        assert!(canvas_row(&canvas, swap_row + 2).contains("25%"));
        assert!(output[output.find("Swap:").unwrap()..].contains("Free:"));
    }

    #[test]
    fn normal_disk_header_shows_live_read_and_write_directions() {
        let mut app = app();
        app.sample.memory.disks.push(DiskSample {
            mount: "/".into(),
            total: 2 * 1024 * 1024 * 1024,
            used: 1024 * 1024 * 1024,
            free: 1024 * 1024 * 1024,
            io_supported: true,
            read_per_second: 1024 * 1024,
            write_per_second: 2 * 1024 * 1024,
            io_activity: 25.0,
        });
        app.disk_histories.insert(
            "/".into(),
            DiskHistory {
                activity: VecDeque::from([0.0, 25.0]),
                ..DiskHistory::default()
            },
        );
        let mut canvas = Canvas::new(80, 20);

        draw_memory(&mut canvas, Rect::new(0, 0, 80, 20), &mut app);

        assert!(canvas_text(&canvas).contains("▼▲3.0M"));
    }

    #[test]
    fn zero_disk_activity_does_not_draw_an_io_percent_floor() {
        let mut app = app();
        app.sample.memory.disks.push(DiskSample {
            mount: "/".into(),
            total: 2 * 1024 * 1024 * 1024,
            used: 1024 * 1024 * 1024,
            free: 1024 * 1024 * 1024,
            io_supported: true,
            ..DiskSample::default()
        });
        app.disk_histories.insert(
            "/".into(),
            DiskHistory {
                activity: VecDeque::from([0.0]),
                ..DiskHistory::default()
            },
        );
        let mut canvas = Canvas::new(80, 20);

        draw_memory(&mut canvas, Rect::new(0, 0, 80, 20), &mut app);

        let (row, label_x) = (0..20)
            .find_map(|y| {
                let cells = &canvas.cells[y * 80..(y + 1) * 80];
                (0..78)
                    .find(|&x| {
                        cells[x].ch == 'I' && cells[x + 1].ch == 'O' && cells[x + 2].ch == '%'
                    })
                    .map(|x| (y, x))
            })
            .expect("disk IO% row");
        assert!(
            canvas.cells[row * 80 + label_x + 5..row * 80 + 79]
                .iter()
                .all(|cell| cell.ch == ' ' || !matches!(cell.style, theme::Style::Available(_)))
        );
        assert!(
            canvas.cells[row * 80 + label_x + 5..row * 80 + 79]
                .iter()
                .any(|cell| cell.style == theme::LOW && cell.ch != ' ')
        );
    }

    #[test]
    fn disk_direction_field_keeps_the_rate_at_a_fixed_column() {
        let read = disk_io_label(3 * 1024 * 1024, 0, false).unwrap();
        let write = disk_io_label(0, 3 * 1024 * 1024, false).unwrap();
        let both = disk_io_label(1024 * 1024, 2 * 1024 * 1024, false).unwrap();
        assert_eq!(read, " ▲3.0M");
        assert_eq!(write, " ▼3.0M");
        assert_eq!(both, "▼▲3.0M");
        assert_eq!(units::display_width(&read), units::display_width(&both));
        assert_eq!(units::display_width(&write), units::display_width(&both));
    }

    #[test]
    fn compact_disk_panel_uses_the_last_available_row() {
        let mut app = app();
        app.sample.memory.disks = ["/", "/boot/efi"]
            .into_iter()
            .map(|mount| DiskSample {
                mount: mount.into(),
                total: 1024 * 1024 * 1024,
                used: 512 * 1024 * 1024,
                free: 512 * 1024 * 1024,
                io_supported: true,
                ..DiskSample::default()
            })
            .collect();
        app.disk_histories
            .insert("/".into(), DiskHistory::default());
        app.disk_histories
            .insert("/boot/efi".into(), DiskHistory::default());
        let mut canvas = Canvas::new(54, 10);
        draw_memory(&mut canvas, Rect::new(0, 0, 54, 10), &mut app);
        let output = canvas_text(&canvas);
        assert!(output.contains("root"));
        assert!(output.contains("efi"));
        assert!(!canvas_row(&canvas, 9).contains("Free:"));
    }

    #[test]
    fn disk_io_speed_overrides_are_in_mebibytes() {
        assert_eq!(
            disk_io_speeds("/:10 /home:250"),
            HashMap::from([
                ("/".to_string(), 10 * 1024 * 1024),
                ("/home".to_string(), 250 * 1024 * 1024),
            ])
        );
    }

    #[test]
    fn title_separators_use_the_panel_line_color() {
        let mut canvas = Canvas::new(30, 5);
        canvas.panel(Rect::new(0, 0, 30, 5), "¹cpu", theme::CPU_BOX, None);
        canvas.title(10, 0, "menu", theme::CPU_BOX);

        assert_eq!(canvas.cells[2].ch, '┐');
        assert_eq!(canvas.cells[2].style, theme::CPU_BOX);
        assert_eq!(canvas.cells[3].style, theme::HI);
        assert!(canvas.cells[3].bold);
        assert_eq!(canvas.cells[4].style, theme::TITLE);
        assert!(canvas.cells[4].bold);
        assert_eq!(canvas.cells[10].ch, '┐');
        assert_eq!(canvas.cells[10].style, theme::CPU_BOX);
        assert_eq!(canvas.cells[11].style, theme::TITLE);
        assert!(canvas.cells[11].bold);
        assert_eq!(canvas.cells[15].ch, '┌');
        assert_eq!(canvas.cells[15].style, theme::CPU_BOX);
    }

    #[test]
    fn theme_cycle_matches_configured_stem_or_filename() {
        let choices = vec![
            "Default".to_string(),
            "TTY".to_string(),
            "dracula.theme".to_string(),
            "nord.theme".to_string(),
        ];
        let mut config = Config::default();
        config.set_value("color_theme", "dracula");
        cycle_theme(&mut config, &choices, 1);
        assert_eq!(config.color_theme, "nord.theme");
    }

    #[test]
    fn options_theme_change_repaints_the_next_frame() {
        let mut app = app();
        let mut renderer = Renderer::new();

        app.handle_key(Key::Char('o'));
        app.handle_key(Key::Right);
        assert_eq!(app.config.color_theme, "TTY");
        let tty = renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );
        assert_eq!(renderer.theme_name, "TTY");
        assert!(tty.contains("\x1b[90;40m"));

        app.handle_key(Key::Right);
        let first_file_theme = theme_choices(&app.config)[2].clone();
        assert_eq!(app.config.color_theme, first_file_theme);
        let themed = renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );
        assert_eq!(renderer.theme_name, first_file_theme);
        assert!(themed.contains("\x1b[38;2;"));
    }

    #[test]
    fn process_tree_prefixes_preserve_branches_and_parent_markers() {
        let process = |pid, parent| ProcessSample {
            pid,
            parent,
            ..ProcessSample::default()
        };
        let processes = [process(1, 1), process(2, 1), process(3, 1), process(4, 3)];
        let collapsed = HashSet::new();
        let listed = tree_processes(
            processes.iter().collect(),
            ProcessSort::Pid,
            false,
            &collapsed,
        );
        let parents = HashSet::from([1, 3]);

        assert_eq!(
            process_tree_prefix(&listed, 0, &parents, &collapsed),
            "[-]─"
        );
        assert_eq!(
            process_tree_prefix(&listed, 1, &parents, &collapsed),
            " │ [-]─"
        );
        assert_eq!(
            process_tree_prefix(&listed, 2, &parents, &collapsed),
            " │  │  └─"
        );
        assert_eq!(
            process_tree_prefix(&listed, 3, &parents, &collapsed),
            " │  └─"
        );
    }

    #[test]
    fn process_filters_match_btop_substring_and_extended_regex_rules() {
        let process = ProcessSample {
            pid: 42,
            name: "systemd-journald".into(),
            command: "/usr/lib/systemd/systemd-journald --namespace main".into(),
            user: "root".into(),
            ..ProcessSample::default()
        };
        assert!(matches_process_filter(&process, "JOURNAL"));
        assert!(matches_process_filter(&process, "42"));
        assert!(matches_process_filter(&process, "!^systemd"));
        assert!(!matches_process_filter(&process, "!--namespace"));
        assert!(matches_process_filter(&process, "!.*--namespace.*"));
        assert!(!matches_process_filter(&process, "!["));
        assert_eq!(clip_text("systemd-timesyncd", 7), "systemd");
        assert_eq!(clip_text_with_plus("systemd-timesync", 5), "syst+");
        assert_eq!(sanitize_ascii_control("one\ntwo\tthree"), "one two three");
    }

    #[test]
    fn process_pid_and_cpu_lazy_sorting_match_reference_order() {
        let processes = (0..8)
            .map(|index| ProcessSample {
                pid: index + 1,
                cumulative_cpu: (8 - index) as f64,
                cpu: if index == 7 { 50.0 } else { 0.0 },
                ..ProcessSample::default()
            })
            .collect::<Vec<_>>();
        let mut refs = processes.iter().collect::<Vec<_>>();
        refs.sort_by(|a, b| compare_process(a, b, ProcessSort::Pid));
        assert_eq!(refs[0].pid, 8);

        refs.sort_by(|a, b| compare_process(a, b, ProcessSort::CpuLazy));
        promote_busy_processes(&mut refs);
        assert_eq!(refs[0].pid, 8);
        assert_eq!(refs[1].pid, 1);
    }

    #[test]
    fn process_tree_filters_keep_matches_and_their_descendants() {
        let root = ProcessSample {
            pid: 1,
            parent: 1,
            name: "init".into(),
            ..ProcessSample::default()
        };
        let parent = ProcessSample {
            pid: 20,
            parent: 1,
            name: "shell".into(),
            ..ProcessSample::default()
        };
        let child = ProcessSample {
            pid: 30,
            parent: 20,
            name: "needle".into(),
            ..ProcessSample::default()
        };
        let unrelated = ProcessSample {
            pid: 40,
            parent: 1,
            name: "other".into(),
            ..ProcessSample::default()
        };
        let grandchild = ProcessSample {
            pid: 31,
            parent: 30,
            name: "worker".into(),
            ..ProcessSample::default()
        };
        let filtered = tree_filter_with_descendants(
            &[&root, &parent, &child, &grandchild, &unrelated],
            "needle",
        );
        assert_eq!(
            filtered
                .iter()
                .map(|process| process.pid)
                .collect::<Vec<_>>(),
            vec![30, 31]
        );
    }

    #[test]
    fn collapsed_and_aggregate_tree_rows_include_descendant_resources() {
        let mut processes = vec![
            ProcessSample {
                pid: 1,
                parent: 1,
                cpu: 1.0,
                cumulative_cpu: 2.0,
                memory: 100,
                threads: 1,
                ..ProcessSample::default()
            },
            ProcessSample {
                pid: 2,
                parent: 1,
                cpu: 3.0,
                cumulative_cpu: 4.0,
                memory: 200,
                threads: 2,
                ..ProcessSample::default()
            },
            ProcessSample {
                pid: 3,
                parent: 2,
                cpu: 5.0,
                cumulative_cpu: 6.0,
                memory: 300,
                threads: 3,
                ..ProcessSample::default()
            },
        ];
        aggregate_tree_resources(&mut processes, false, &HashSet::from([1]));
        assert_eq!(processes[0].cpu, 9.0);
        assert_eq!(processes[0].cumulative_cpu, 12.0);
        assert_eq!(processes[0].memory, 600);
        assert_eq!(processes[0].threads, 6);
        assert_eq!(processes[1].memory, 200);

        aggregate_tree_resources(&mut processes, true, &HashSet::new());
        assert_eq!(processes[1].memory, 500);
    }

    #[test]
    fn tree_auto_collapse_skips_root_and_direct_children() {
        let process = |pid, parent| ProcessSample {
            pid,
            parent,
            ..ProcessSample::default()
        };
        let processes = vec![
            process(1, 0),
            process(2, 1),
            process(3, 2),
            process(4, 3),
            process(5, 3),
        ];
        let mut collapsed = HashSet::new();
        auto_collapse_oversized(&processes, 2, &mut collapsed);
        assert_eq!(collapsed, HashSet::from([3]));
    }

    #[test]
    fn repeated_tree_collapse_controls_match_source_transitions_while_paused() {
        let process = |pid, parent| ProcessSample {
            pid,
            parent,
            ..ProcessSample::default()
        };
        let mut app = app();
        app.config.process_tree = true;
        app.config.pause_processes = true;
        app.sample.processes = vec![process(1, 0), process(2, 1), process(3, 2), process(4, 1)];
        app.visible_pids = vec![1, 2, 3, 4];
        app.process_selected = true;
        app.selected_process = 1;

        app.handle_key(Key::Char('-'));
        app.handle_key(Key::Char('-'));
        assert_eq!(app.collapsed_processes, HashSet::from([2]));
        app.handle_key(Key::Char('+'));
        app.handle_key(Key::Char('+'));
        assert!(app.collapsed_processes.is_empty());

        app.handle_key(Key::Char('C'));
        assert_eq!(app.collapsed_processes, HashSet::from([3]));
        app.handle_key(Key::Char('C'));
        assert!(app.collapsed_processes.is_empty());

        app.handle_key(Key::Char('E'));
        assert_eq!(app.collapsed_processes, HashSet::from([2, 3, 4]));
        app.handle_key(Key::Char('E'));
        assert!(app.collapsed_processes.is_empty());
    }

    #[test]
    fn clicking_a_tree_marker_collapses_that_exact_process() {
        let process = |pid, parent, name: &str| ProcessSample {
            pid,
            parent,
            name: name.into(),
            ..ProcessSample::default()
        };
        let mut app = app();
        app.config.process_tree = true;
        app.config.set_value("shown_boxes", "cpu gpu0 mem net proc");
        app.sample.gpus.push(GpuSample::default());
        app.gpu_histories.push(GpuHistory::default());
        app.sample.processes = vec![
            process(1, 1, "root"),
            process(2, 1, "first-parent"),
            process(3, 2, "first-child"),
            process(4, 1, "second-parent"),
            process(5, 4, "second-child"),
        ];
        let mut renderer = Renderer::new();
        renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );
        let marker = app
            .process_hitboxes
            .iter()
            .find(|hitbox| hitbox.pid == 4)
            .copied()
            .expect("second parent is visible");
        let (x, _) = marker.toggle_x.expect("parent has a toggle marker");

        app.handle_key(Key::Mouse {
            button: 0,
            x: x as u16,
            y: marker.y as u16,
            pressed: true,
        });
        assert_eq!(app.collapsed_processes, HashSet::from([4]));

        renderer.render(
            Size {
                cols: 120,
                rows: 40,
            },
            &mut app,
        );
        assert!(!app.visible_pids.contains(&5));
        assert!(app.visible_pids.contains(&3));
        let collapsed_marker = app
            .process_hitboxes
            .iter()
            .find(|hitbox| hitbox.pid == 4)
            .copied()
            .expect("collapsed parent remains visible");
        let listed = tree_processes(
            app.sample.processes.iter().collect(),
            ProcessSort::Pid,
            false,
            &app.collapsed_processes,
        );
        let index = listed
            .iter()
            .position(|(process, _)| process.pid == 4)
            .unwrap();
        assert_eq!(
            process_tree_prefix(
                &listed,
                index,
                &HashSet::from([1, 2, 4]),
                &app.collapsed_processes,
            ),
            " │ [+]─"
        );
        assert_eq!(collapsed_marker.pid, 4);
    }

    #[test]
    fn process_scrollbar_is_drawn_and_can_be_dragged() {
        let mut app = app();
        app.config.process_tree = false;
        app.sample.processes = (1..=40)
            .map(|pid| ProcessSample {
                pid,
                name: format!("process-{pid}"),
                ..ProcessSample::default()
            })
            .collect();
        let mut canvas = Canvas::new(80, 15);
        draw_processes(&mut canvas, Rect::new(0, 0, 80, 15), &mut app);
        let scrollbar = app.process_scrollbar.expect("long lists have a scrollbar");
        assert_eq!(
            canvas.cells[scrollbar.up_y * canvas.width + scrollbar.x].ch,
            '↑'
        );
        assert_eq!(
            canvas.cells[scrollbar.thumb_y * canvas.width + scrollbar.x].ch,
            '█'
        );

        app.handle_key(Key::Mouse {
            button: 0,
            x: scrollbar.x as u16,
            y: scrollbar.thumb_y as u16,
            pressed: true,
        });
        app.handle_key(Key::Mouse {
            button: 32,
            x: scrollbar.x as u16,
            y: scrollbar.track_bottom.saturating_sub(1) as u16,
            pressed: true,
        });
        assert_eq!(app.selected_process, 39);
    }

    #[test]
    fn network_stats_do_not_overwrite_their_box_border() {
        let mut app = app();
        app.sample.network.connected = true;
        app.sample.network.selected = "eth0".into();
        app.sample.network.download_per_second = 36 * 1_024;
        app.sample.network.upload_per_second = 6_820;
        app.download_top = 1_875_000;
        app.upload_top = 1_750_000;
        let mut canvas = Canvas::new(50, 20);

        draw_network(&mut canvas, Rect::new(0, 0, 50, 20), &mut app);

        let stats_x = 50 - 27 - 1;
        let stats_y = (20usize.saturating_sub(2) / 2) - 9 / 2 + 1;
        let right_border = stats_x + 27 - 1;
        for y in [
            stats_y + 1,
            stats_y + 2,
            stats_y + 3,
            stats_y + 5,
            stats_y + 6,
        ] {
            assert_eq!(canvas.cells[y * canvas.width + right_border].ch, '│');
        }

        let mut minimum = Canvas::new(36, 6);
        draw_network(&mut minimum, Rect::new(0, 0, 36, 6), &mut app);
        assert!(canvas_row(&minimum, 2).contains('▼'));
        assert!(canvas_row(&minimum, 3).contains('▲'));
        assert_eq!(minimum.cells[2 * 36 + 34].ch, '│');
        assert_eq!(minimum.cells[3 * 36 + 34].ch, '│');
    }

    #[test]
    fn network_title_controls_are_stateful_mouse_targets() {
        let mut app = app();
        app.config.set_value("shown_boxes", "net");
        app.sample.network.connected = true;
        app.sample.network.selected = "eth0".into();
        app.sample.network.interfaces = vec!["eth0".into(), "wlan0".into()];
        app.sample.network.downloaded = 12_345;
        app.sample.network.uploaded = 6_789;
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 120,
            rows: 40,
        };
        renderer.render(size, &mut app);

        let click = |app: &mut AppState, hitbox: NetworkHitbox| {
            app.handle_key(Key::Mouse {
                button: 0,
                x: hitbox.start as u16,
                y: hitbox.y as u16,
                pressed: true,
            });
        };
        let hitbox = |app: &AppState, action| {
            app.network_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == action)
                .copied()
                .expect("network control is visible")
        };

        let sync = hitbox(&app, NetworkAction::Sync);
        click(&mut app, sync);
        assert!(!app.config.net_sync);
        click(&mut app, sync);
        assert!(app.config.net_sync);
        let auto = hitbox(&app, NetworkAction::Auto);
        click(&mut app, auto);
        assert!(!app.config.net_auto);
        click(&mut app, auto);
        assert!(app.config.net_auto);
        let previous = hitbox(&app, NetworkAction::Previous);
        click(&mut app, previous);
        assert_eq!(app.config.net_iface.as_deref(), Some("wlan0"));

        let zero = hitbox(&app, NetworkAction::Zero);
        click(&mut app, zero);
        assert!(app.network_zero_active());
        assert_eq!(app.sample.network.downloaded, 0);
        assert_eq!(app.sample.network.uploaded, 0);
        renderer.render(size, &mut app);
        let zero = hitbox(&app, NetworkAction::Zero);
        click(&mut app, zero);
        assert!(!app.network_zero_active());
        assert_eq!(app.sample.network.downloaded, 12_345);
        assert_eq!(app.sample.network.uploaded, 6_789);

        let mut canvas = Canvas::new(60, 20);
        app.config.net_sync = true;
        app.config.net_auto = true;
        draw_network(&mut canvas, Rect::new(0, 0, 60, 20), &mut app);
        for action in [NetworkAction::Sync, NetworkAction::Auto] {
            let control = hitbox(&app, action);
            assert!(canvas.cells[control.y * canvas.width + control.start].bold);
        }
        let zero = hitbox(&app, NetworkAction::Zero);
        assert!(!canvas.cells[zero.y * canvas.width + zero.start].bold);

        app.sample.network.selected = "enp5s0".into();
        app.sample.network.ipv4 = Some("10.0.0.100".into());
        let mut narrow = Canvas::new(54, 12);
        draw_network(&mut narrow, Rect::new(0, 0, 54, 12), &mut app);
        let title = canvas_row(&narrow, 0);
        assert!(title.contains("10.0.0.100"));
        assert!(title.contains("sync"));
    }

    #[test]
    fn disconnected_network_interface_keeps_its_selector() {
        let mut app = app();
        app.config.set_value("shown_boxes", "net");
        app.sample.network.interfaces = vec!["down0".into(), "eth0".into()];
        app.sample.network.selected = "down0".into();
        app.sample.network.connected = false;
        let mut renderer = Renderer::new();
        let frame = renderer.render(
            Size {
                cols: 100,
                rows: 30,
            },
            &mut app,
        );

        assert!(frame.contains("down0"));
        let next = app
            .network_hitboxes
            .iter()
            .find(|hitbox| hitbox.action == NetworkAction::Next)
            .copied()
            .expect("disconnected interfaces retain the selector");
        app.handle_key(Key::Mouse {
            button: 0,
            x: next.start as u16,
            y: next.y as u16,
            pressed: true,
        });
        assert_eq!(app.config.net_iface.as_deref(), Some("eth0"));
    }

    #[test]
    fn process_header_labels_are_stateful_mouse_controls() {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 120,
            rows: 40,
        };
        renderer.render(size, &mut app);
        let hitbox = |app: &AppState, action| {
            app.process_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == action)
                .copied()
                .expect("process header control is visible")
        };
        let click = |app: &mut AppState, hitbox: ProcessControlHitbox| {
            app.handle_key(Key::Mouse {
                button: 0,
                x: hitbox.start as u16,
                y: hitbox.y as u16,
                pressed: true,
            });
        };

        for (action, value) in [
            (ProcessControlAction::Pause, true),
            (ProcessControlAction::PerCore, true),
            (ProcessControlAction::Reverse, true),
            (ProcessControlAction::Tree, true),
        ] {
            let control = hitbox(&app, action);
            click(&mut app, control);
            let actual = match action {
                ProcessControlAction::Pause => app.config.pause_processes,
                ProcessControlAction::PerCore => app.config.process_per_core,
                ProcessControlAction::Reverse => app.config.process_reversed,
                ProcessControlAction::Tree => app.config.process_tree,
                _ => unreachable!(),
            };
            assert_eq!(actual, value);
        }
        for action in [
            ProcessControlAction::Pause,
            ProcessControlAction::PerCore,
            ProcessControlAction::Reverse,
            ProcessControlAction::Tree,
        ] {
            let control = hitbox(&app, action);
            click(&mut app, control);
            let actual = match action {
                ProcessControlAction::Pause => app.config.pause_processes,
                ProcessControlAction::PerCore => app.config.process_per_core,
                ProcessControlAction::Reverse => app.config.process_reversed,
                ProcessControlAction::Tree => app.config.process_tree,
                _ => unreachable!(),
            };
            assert!(!actual, "{action:?} clicks back off");
        }

        let next = hitbox(&app, ProcessControlAction::SortNext);
        click(&mut app, next);
        assert_eq!(app.config.process_sort, ProcessSort::Pid);
        let filter = hitbox(&app, ProcessControlAction::Filter);
        click(&mut app, filter);
        assert!(app.filter_editing);
    }

    #[test]
    fn process_filter_title_edits_and_deletes_the_rendered_filter() {
        let mut app = app();
        app.config.process_filter = "needle".into();
        app.filter_buffer = "needle".into();
        let mut canvas = Canvas::new(100, 20);

        draw_processes(&mut canvas, Rect::new(0, 0, 100, 20), &mut app);
        assert!(canvas_row(&canvas, 0).contains("f needle del"));
        let delete = app
            .process_control_hitboxes
            .iter()
            .find(|hitbox| hitbox.action == ProcessControlAction::DeleteFilter)
            .copied()
            .expect("the rendered del label is clickable");
        app.handle_key(Key::Mouse {
            button: 0,
            x: delete.start as u16,
            y: delete.y as u16,
            pressed: true,
        });
        assert!(app.config.process_filter.is_empty());

        app.config.process_filter = "needle".into();
        app.filter_buffer = "needle".into();
        app.filter_editing = true;
        let mut editing = Canvas::new(100, 20);
        draw_processes(&mut editing, Rect::new(0, 0, 100, 20), &mut app);
        assert!(canvas_row(&editing, 0).contains("f needle  ↵"));
        assert_eq!(
            editing.cells[..100]
                .iter()
                .filter(|cell| cell.underline)
                .count(),
            1
        );
        assert!(
            !app.process_control_hitboxes
                .iter()
                .any(|hitbox| hitbox.action == ProcessControlAction::DeleteFilter)
        );
    }

    #[test]
    fn cpu_and_memory_title_labels_use_rendered_mouse_regions() {
        let mut cpu_app = app();
        cpu_app.config.set_value("shown_boxes", "cpu");
        let mut renderer = Renderer::new();
        let size = Size {
            cols: 120,
            rows: 40,
        };
        renderer.render(size, &mut cpu_app);
        let cpu_hitbox = |app: &AppState, action| {
            app.cpu_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == action)
                .copied()
                .expect("CPU title control is visible")
        };
        let click_cpu = |app: &mut AppState, hitbox: CpuControlHitbox| {
            app.handle_key(Key::Mouse {
                button: 0,
                x: hitbox.start as u16,
                y: hitbox.y as u16,
                pressed: true,
            });
        };
        let decrease = cpu_hitbox(&cpu_app, CpuControlAction::DecreaseUpdate);
        click_cpu(&mut cpu_app, decrease);
        assert_eq!(cpu_app.config.update_ms, 1_900);
        let increase = cpu_hitbox(&cpu_app, CpuControlAction::IncreaseUpdate);
        click_cpu(&mut cpu_app, increase);
        assert_eq!(cpu_app.config.update_ms, 2_000);
        let preset = cpu_hitbox(&cpu_app, CpuControlAction::Preset);
        click_cpu(&mut cpu_app, preset);
        assert_eq!(cpu_app.config.preset, Some(0));
        let menu = cpu_hitbox(&cpu_app, CpuControlAction::Menu);
        click_cpu(&mut cpu_app, menu);
        assert_eq!(cpu_app.overlay, Overlay::Main { selected: 0 });

        let mut mem_app = app();
        mem_app.config.set_value("shown_boxes", "mem");
        renderer.render(size, &mut mem_app);
        let memory_hitbox = |app: &AppState, action| {
            app.memory_control_hitboxes
                .iter()
                .find(|hitbox| hitbox.action == action)
                .copied()
                .expect("memory title control is visible")
        };
        let click_memory = |app: &mut AppState, hitbox: MemoryControlHitbox| {
            app.handle_key(Key::Mouse {
                button: 0,
                x: hitbox.start as u16,
                y: hitbox.y as u16,
                pressed: true,
            });
        };
        let io = memory_hitbox(&mem_app, MemoryControlAction::IoMode);
        click_memory(&mut mem_app, io);
        assert_eq!(mem_app.config.bool_value("io_mode"), Some(true));
        click_memory(&mut mem_app, io);
        assert_eq!(mem_app.config.bool_value("io_mode"), Some(false));
        let disks = memory_hitbox(&mem_app, MemoryControlAction::Disks);
        click_memory(&mut mem_app, disks);
        assert!(!mem_app.config.show_disks);
        click_memory(&mut mem_app, disks);
        assert!(mem_app.config.show_disks);
    }
}
