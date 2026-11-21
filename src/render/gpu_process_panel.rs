use super::gpu_process_view::{GpuRowsContext, draw_gpu_process_rows};
use super::*;

#[derive(Default)]
pub(super) struct GpuProcessPanel {
    pub processes: Vec<ProcessSample>,
    pub selected: usize,
    pub has_selection: bool,
    pub offset: usize,
    pub followed: Option<u32>,
    pub collapsed: HashSet<u32>,
    pub tree: bool,
    pub hits: Vec<ProcessHitbox>,
    pub cpu_histories: HashMap<u32, VecDeque<f64>>,
    pub detail: Option<ProcessSample>,
    pub detail_cpu_history: VecDeque<f64>,
    pub detail_memory_history: VecDeque<f64>,
    pub area: Option<Rect>,
    pub visible: Vec<u32>,
    pub editing_filter: bool,
    filter_buffer: String,
    pub(super) controls: Vec<ProcessControlHitbox>,
    scrollbar: Option<ProcessScrollbar>,
    pub(super) dragging: bool,
    pub(super) paused: bool,
    rows: usize,
}

pub(super) struct GpuProcessRows {
    rows: Vec<(ProcessSample, usize)>,
    parents: HashSet<u32>,
}

impl GpuProcessRows {
    pub fn height_within(&self, maximum: usize, details: bool) -> usize {
        let (detail_rows, minimum) = if details && maximum >= 14 {
            (8, 14)
        } else if details && maximum >= 8 {
            (4, 8)
        } else {
            (0, 4)
        };
        (self.rows.len().max(1) + 3 + detail_rows)
            .max(minimum)
            .min(maximum)
    }
}

impl GpuProcessPanel {
    pub fn listed_processes(&self, config: &Config) -> GpuProcessRows {
        let tree = config.bool_value("gpu_proc_tree").unwrap_or(false);
        let reversed = config.bool_value("gpu_proc_reversed").unwrap_or(false);
        let sort = sorting(config);
        let filter = if self.editing_filter {
            self.filter_buffer.as_str()
        } else {
            config.value("gpu_proc_filter").unwrap_or("")
        };
        let gpu_pids = gpu_process_filter(&self.processes, tree);
        let mut display: Vec<_> = self
            .processes
            .iter()
            .filter(|process| gpu_pids.contains(&process.pid))
            .cloned()
            .collect();
        if tree {
            aggregate_tree_resources(
                &mut display,
                config.bool_value("proc_aggregate").unwrap_or(false),
                &self.collapsed,
            );
        }
        let eligible: Vec<_> = display.iter().collect();
        let mut processes = if tree && !filter.is_empty() {
            tree_filter_with_descendants(&eligible, filter)
        } else {
            eligible
                .into_iter()
                .filter(|process| matches_process_filter(process, filter))
                .collect()
        };
        sort_process_group(&mut processes, sort, reversed);
        let pids: HashSet<_> = processes.iter().map(|process| process.pid).collect();
        let parents = processes
            .iter()
            .filter(|process| process.pid != process.parent && pids.contains(&process.parent))
            .map(|process| process.parent)
            .collect();
        let listed = if tree {
            tree_processes(processes, sort, reversed, &self.collapsed)
        } else {
            processes.into_iter().map(|process| (process, 0)).collect()
        };
        GpuProcessRows {
            rows: listed
                .into_iter()
                .map(|(process, depth)| (process.clone(), depth))
                .collect(),
            parents,
        }
    }

    pub fn selected_pid(&self) -> Option<u32> {
        self.has_selection
            .then(|| self.visible.get(self.selected).copied())
            .flatten()
    }

    pub fn clear_regions(&mut self) {
        self.area = None;
        self.hits.clear();
        self.controls.clear();
        self.scrollbar = None;
    }

    pub fn update(&mut self, current: &[ProcessSample], config: &Config, cores: usize) {
        let keep_usage = config.bool_value("keep_dead_proc_usage").unwrap_or(false);
        let identities: HashMap<_, _> = current
            .iter()
            .map(|process| (process.pid, process.start_time))
            .collect();
        let same_process = |pid| {
            self.processes
                .iter()
                .find(|process| process.pid == pid)
                .is_some_and(|previous| {
                    identities
                        .get(&pid)
                        .is_some_and(|start| *start == previous.start_time)
                })
        };
        if self.selected_pid().is_some_and(|pid| !same_process(pid)) {
            self.has_selection = false;
        }
        if self.followed.is_some_and(|pid| !same_process(pid)) {
            self.followed = None;
        }
        self.collapsed.retain(|pid| same_process(*pid));
        self.cpu_histories.retain(|pid, _| same_process(*pid));
        if self.detail.as_ref().is_some_and(|detail| {
            identities
                .get(&detail.pid)
                .is_some_and(|start| *start != detail.start_time)
        }) {
            self.detail = None;
            self.detail_cpu_history.clear();
            self.detail_memory_history.clear();
        }
        if self.paused {
            for process in &mut self.processes {
                if identities.get(&process.pid) != Some(&process.start_time) {
                    process.state = 'X';
                    if !keep_usage {
                        process.cpu = 0.0;
                        process.memory = 0;
                        process.gpu.bytes = Some(0);
                        process.gpu.utilization = Some(0);
                        for device in &mut process.gpu.devices {
                            device.bytes = Some(0);
                            device.utilization = Some(0);
                        }
                    }
                }
            }
        } else {
            self.processes = current.to_vec();
            let gpu_pids = gpu_process_filter(current, self.tree);
            self.cpu_histories.retain(|pid, _| gpu_pids.contains(pid));
            for process in current
                .iter()
                .filter(|process| gpu_pids.contains(&process.pid))
            {
                let value = if (0.1..5.0).contains(&process.cpu) {
                    5.0
                } else {
                    process.cpu.round()
                };
                push_history(
                    self.cpu_histories.entry(process.pid).or_default(),
                    value,
                    MIN_HISTORY,
                );
            }
        }
        if let Some(detail) = &mut self.detail {
            if let Some(process) = self
                .processes
                .iter()
                .find(|process| process.pid == detail.pid)
            {
                *detail = process.clone();
            } else {
                detail.state = 'X';
            }
            if !matches!(detail.state, 'X' | 'x') {
                let cpu = if config.process_per_core {
                    detail.cpu
                } else {
                    detail.cpu * cores as f64
                };
                push_history(&mut self.detail_cpu_history, cpu.min(100.0), MIN_HISTORY);
                push_history(
                    &mut self.detail_memory_history,
                    detail.memory as f64,
                    MIN_HISTORY,
                );
            }
        }
    }

    fn select(&mut self, delta: isize) {
        if !self.has_selection
            && let Some(pid) = self.followed
            && let Some(index) = self.visible.iter().position(|candidate| *candidate == pid)
        {
            self.selected = index;
            self.has_selection = true;
        }
        if !self.paused {
            self.followed = None;
        }
        if self.visible.is_empty() {
            self.has_selection = false;
            return;
        }
        if delta < 0 && (!self.has_selection || self.selected < delta.unsigned_abs()) {
            self.selected = 0;
            self.has_selection = false;
            return;
        }
        self.selected = if !self.has_selection {
            0
        } else {
            self.selected
                .saturating_add_signed(delta)
                .min(self.visible.len() - 1)
        };
        self.has_selection = true;
    }
}

fn sorting(config: &Config) -> ProcessSort {
    ProcessSort::GPU_ALL
        .into_iter()
        .find(|sort| Some(sort.label()) == config.value("gpu_proc_sorting"))
        .unwrap_or(ProcessSort::GpuMemory)
}

impl AppState {
    pub(super) fn gpu_process_edit_key(&mut self, key: Key) -> bool {
        if !self.gpu_process_panel.editing_filter {
            return false;
        }
        let panel = &mut self.gpu_process_panel;
        match key {
            Key::Enter => {
                self.config
                    .set_value("gpu_proc_filter", panel.filter_buffer.clone());
                panel.editing_filter = false;
            }
            Key::Escape => panel.editing_filter = false,
            Key::Backspace => {
                panel.filter_buffer.pop();
            }
            Key::Delete => panel.filter_buffer.clear(),
            Key::Char(ch) if !ch.is_control() => panel.filter_buffer.push(ch),
            _ => return true,
        }
        panel.offset = 0;
        panel.has_selection = false;
        self.needs_redraw = true;
        true
    }

    pub(super) fn gpu_process_key(&mut self, key: Key) -> bool {
        if !self.gpu_processes_visible() || !self.gpu_process_focus {
            return false;
        }
        match key {
            Key::Up => self.gpu_process_panel.select(-1),
            Key::Down => self.gpu_process_panel.select(1),
            Key::Char('k') if self.config.vim_keys => self.gpu_process_panel.select(-1),
            Key::Char('j') if self.config.vim_keys => self.gpu_process_panel.select(1),
            Key::PageUp => self
                .gpu_process_panel
                .select(-(self.gpu_process_panel.rows.max(1) as isize)),
            Key::PageDown => self
                .gpu_process_panel
                .select(self.gpu_process_panel.rows.max(1) as isize),
            Key::Home | Key::Char('g') if key == Key::Home || self.config.vim_keys => {
                if !self.gpu_process_panel.paused {
                    self.gpu_process_panel.followed = None;
                }
                self.gpu_process_panel.selected = 0;
                self.gpu_process_panel.has_selection = !self.gpu_process_panel.visible.is_empty();
            }
            Key::End | Key::Char('G') if key == Key::End || self.config.vim_keys => {
                if !self.gpu_process_panel.paused {
                    self.gpu_process_panel.followed = None;
                }
                self.gpu_process_panel.selected =
                    self.gpu_process_panel.visible.len().saturating_sub(1);
                self.gpu_process_panel.has_selection = !self.gpu_process_panel.visible.is_empty();
            }
            Key::Left => self.gpu_process_control(ProcessControlAction::SortPrevious),
            Key::Right => self.gpu_process_control(ProcessControlAction::SortNext),
            Key::Enter => self.gpu_process_control(ProcessControlAction::Info),
            Key::Char('f' | '/') => self.gpu_process_control(ProcessControlAction::Filter),
            Key::Delete => self.gpu_process_control(ProcessControlAction::DeleteFilter),
            Key::Char('r') => self.gpu_process_control(ProcessControlAction::Reverse),
            Key::Char('e') => self.gpu_process_control(ProcessControlAction::Tree),
            Key::Char('u') => self.gpu_process_control(ProcessControlAction::Pause),
            Key::Char('F') => self.gpu_process_control(ProcessControlAction::Follow),
            Key::Char('t') => self.open_signal(15),
            Key::Char('k') if !self.config.vim_keys => self.open_signal(9),
            Key::Char('K') if self.config.vim_keys => self.open_signal(9),
            Key::Char('s') => self.open_signal_chooser(),
            Key::Char('N') => self.open_renice(),
            Key::Char('C') => {
                let panel = &mut self.gpu_process_panel;
                if panel.tree
                    && let Some(pid) = panel.selected_pid()
                {
                    for child in panel
                        .processes
                        .iter()
                        .filter(|p| p.parent == pid && p.pid != pid)
                    {
                        if !panel.collapsed.remove(&child.pid) {
                            panel.collapsed.insert(child.pid);
                        }
                    }
                }
            }
            Key::Char('E') => {
                let panel = &mut self.gpu_process_panel;
                if panel.tree {
                    let pids = gpu_process_filter(&panel.processes, true);
                    let parents: HashSet<_> = panel
                        .processes
                        .iter()
                        .filter(|p| {
                            pids.contains(&p.pid) && pids.contains(&p.parent) && p.pid != p.parent
                        })
                        .map(|p| p.parent)
                        .collect();
                    if parents.iter().any(|pid| !panel.collapsed.contains(pid)) {
                        panel.collapsed.extend(parents);
                    } else {
                        panel.collapsed.clear();
                    }
                }
            }
            Key::Char(' ' | '+' | '-' | '=') if self.gpu_process_panel.tree => {
                if let Some(pid) = self.gpu_process_panel.selected_pid() {
                    match key {
                        Key::Char('+') | Key::Char('=') => {
                            self.gpu_process_panel.collapsed.remove(&pid);
                        }
                        Key::Char('-') => {
                            self.gpu_process_panel.collapsed.insert(pid);
                        }
                        _ => {
                            if !self.gpu_process_panel.collapsed.remove(&pid) {
                                self.gpu_process_panel.collapsed.insert(pid);
                            }
                        }
                    }
                }
            }
            Key::Char(' ') => {}
            Key::Char('+' | '=') => self.activate_cpu_control(CpuControlAction::IncreaseUpdate),
            Key::Char('-') => self.activate_cpu_control(CpuControlAction::DecreaseUpdate),
            _ => return false,
        }
        self.needs_redraw = true;
        true
    }

    fn gpu_process_control(&mut self, action: ProcessControlAction) {
        match action {
            ProcessControlAction::SortPrevious | ProcessControlAction::SortNext => {
                let choices = ProcessSort::GPU_ALL;
                let current = choices
                    .iter()
                    .position(|sort| *sort == sorting(&self.config))
                    .unwrap_or(0);
                let next = if action == ProcessControlAction::SortNext {
                    (current + 1) % choices.len()
                } else {
                    (current + choices.len() - 1) % choices.len()
                };
                self.config
                    .set_value("gpu_proc_sorting", choices[next].label());
            }
            ProcessControlAction::Filter => {
                self.gpu_process_panel.filter_buffer = self
                    .config
                    .value("gpu_proc_filter")
                    .unwrap_or("")
                    .to_string();
                self.gpu_process_panel.editing_filter = true;
            }
            ProcessControlAction::DeleteFilter => {
                self.config.set_value("gpu_proc_filter", "");
                self.gpu_process_panel.filter_buffer.clear();
                self.gpu_process_panel.offset = 0;
            }
            ProcessControlAction::Reverse => self.config.flip_value("gpu_proc_reversed"),
            ProcessControlAction::Tree => self.config.flip_value("gpu_proc_tree"),
            ProcessControlAction::Pause => {
                self.gpu_process_panel.paused = !self.gpu_process_panel.paused
            }
            ProcessControlAction::PerCore => self.activate_process_control(action),
            ProcessControlAction::Follow => {
                let pid = self.selected_live_pid();
                self.gpu_process_panel.followed = if self.gpu_process_panel.followed == pid {
                    None
                } else {
                    pid
                };
            }
            ProcessControlAction::Info => {
                let panel = &mut self.gpu_process_panel;
                let pid = panel.selected_pid();
                if panel
                    .detail
                    .as_ref()
                    .is_some_and(|detail| pid.is_none() || pid == Some(detail.pid))
                {
                    panel.detail = None;
                } else if let Some(pid) = pid {
                    panel.detail = panel
                        .processes
                        .iter()
                        .find(|process| process.pid == pid && !matches!(process.state, 'X' | 'x'))
                        .cloned();
                    panel.has_selection = false;
                }
                panel.detail_cpu_history.clear();
                panel.detail_memory_history.clear();
                self.collection_requested = true;
            }
            ProcessControlAction::Terminate => self.open_signal(15),
            ProcessControlAction::Kill => self.open_signal(9),
            ProcessControlAction::Signals => self.open_signal_chooser(),
            ProcessControlAction::Nice => self.open_renice(),
            ProcessControlAction::GpuProcesses => self.activate_process_control(action),
        }
    }

    pub(super) fn gpu_process_mouse(
        &mut self,
        button: u16,
        x: usize,
        y: usize,
        pressed: bool,
    ) -> bool {
        if !self.gpu_processes_visible() {
            self.gpu_process_panel.dragging = false;
            return false;
        }
        if !pressed {
            self.gpu_process_panel.dragging = false;
            return false;
        }
        let in_panel = self
            .gpu_process_panel
            .area
            .is_some_and(|area| area.contains(x, y));
        if !in_panel && !self.gpu_process_panel.dragging {
            return false;
        }
        self.gpu_process_focus = true;
        if let Some(scrollbar) = self.gpu_process_panel.scrollbar
            && ((button == 0
                && x == scrollbar.x
                && (scrollbar.up_y..=scrollbar.down_y).contains(&y))
                || self.gpu_process_panel.dragging && button & 32 != 0)
        {
            let maximum = scrollbar.total.saturating_sub(scrollbar.visible);
            self.gpu_process_panel.offset = if y == scrollbar.up_y {
                self.gpu_process_panel.offset.saturating_sub(1)
            } else if y == scrollbar.down_y {
                (self.gpu_process_panel.offset + 1).min(maximum)
            } else {
                let span = scrollbar
                    .track_bottom
                    .saturating_sub(scrollbar.track_top + 1);
                y.saturating_sub(scrollbar.track_top).min(span) * maximum / span.max(1)
            };
            self.gpu_process_panel.dragging =
                y == scrollbar.thumb_y || self.gpu_process_panel.dragging;
            self.gpu_process_panel.has_selection = false;
            self.gpu_process_panel.followed = None;
            self.needs_redraw = true;
            return true;
        }
        if matches!(button, 64 | 65) {
            let maximum = self
                .gpu_process_panel
                .visible
                .len()
                .saturating_sub(self.gpu_process_panel.rows);
            self.gpu_process_panel.offset = if button == 65 {
                (self.gpu_process_panel.offset + 3).min(maximum)
            } else {
                self.gpu_process_panel.offset.saturating_sub(3)
            };
            self.gpu_process_panel.has_selection = false;
            self.gpu_process_panel.followed = None;
        } else if button == 0 {
            if let Some(action) = self
                .gpu_process_panel
                .controls
                .iter()
                .find(|hit| hit.y == y && (hit.start..hit.end).contains(&x))
                .map(|hit| hit.action)
            {
                self.gpu_process_control(action);
            } else if let Some(hit) = self
                .gpu_process_panel
                .hits
                .iter()
                .find(|hit| hit.y == y)
                .copied()
            {
                let open_info = self.gpu_process_panel.selected_pid() == Some(hit.pid);
                self.gpu_process_panel.selected = hit.index;
                self.gpu_process_panel.has_selection = true;
                if !self.gpu_process_panel.paused {
                    self.gpu_process_panel.followed = None;
                }
                if hit
                    .toggle_x
                    .is_some_and(|(start, end)| (start..end).contains(&x))
                {
                    if !self.gpu_process_panel.collapsed.remove(&hit.pid) {
                        self.gpu_process_panel.collapsed.insert(hit.pid);
                    }
                } else if open_info {
                    self.gpu_process_control(ProcessControlAction::Info);
                }
            } else {
                self.gpu_process_panel.has_selection = false;
            }
        }
        self.needs_redraw = true;
        true
    }
}

pub(super) fn draw_gpu_process_panel(
    canvas: &mut Canvas,
    area: Rect,
    app: &mut AppState,
    processes: GpuProcessRows,
) {
    let panel = &mut app.gpu_process_panel;
    panel.clear_regions();
    panel.area = Some(area);
    let detail_offset = if panel.detail.is_some() && area.h >= 8 {
        if area.h >= 14 { 8 } else { 4 }
    } else {
        0
    };
    let controls_y = area.y + detail_offset;
    canvas.panel(
        area,
        if detail_offset == 0 {
            "ᵛgpu proc"
        } else {
            ""
        },
        theme::PROC_BOX,
        None,
    );
    if area.w < 44 || area.h < 4 {
        return;
    }
    panel.tree = app.config.bool_value("gpu_proc_tree").unwrap_or(false);
    let reversed = app.config.bool_value("gpu_proc_reversed").unwrap_or(false);
    let sort = sorting(&app.config);
    panel.controls.push(ProcessControlHitbox {
        y: controls_y,
        start: area.x + 3,
        end: area.x + 12,
        action: ProcessControlAction::GpuProcesses,
    });
    if detail_offset > 0
        && let Some(detail) = &panel.detail
    {
        draw_process_details(
            canvas,
            area,
            detail,
            ProcessDetailContext {
                config: &app.config,
                sample: &app.sample,
                cpu_history: &panel.detail_cpu_history,
                memory_history: &panel.detail_memory_history,
                has_selection: panel.has_selection,
                selected_pid: panel.selected_pid(),
                followed: panel.followed,
                title: "ᵛgpu proc",
                hitboxes: &mut panel.controls,
            },
        );
    }
    let filter = if panel.editing_filter {
        panel.filter_buffer.as_str()
    } else {
        app.config.value("gpu_proc_filter").unwrap_or("")
    };
    draw_process_header(
        canvas,
        area,
        controls_y,
        ProcessHeader {
            filter_offset: 13,
            filter,
            editing: panel.editing_filter,
            sorting: sort,
            tree: panel.tree,
            reversed,
            per_core: app.config.process_per_core,
            paused: panel.paused,
        },
        &mut panel.controls,
    );
    let listed: Vec<_> = processes
        .rows
        .iter()
        .map(|(process, depth)| (process, *depth))
        .collect();
    let parents = &processes.parents;
    let previous = panel.followed.or(panel.selected_pid());
    panel.visible = listed.iter().map(|(process, _)| process.pid).collect();
    if let Some(pid) = previous {
        if let Some(index) = panel.visible.iter().position(|entry| *entry == pid) {
            panel.selected = index;
        } else {
            panel.has_selection = false;
            panel.followed = None;
        }
    }
    panel.rows = area.h.saturating_sub(detail_offset + 3).max(1);
    panel.offset = panel.offset.min(listed.len().saturating_sub(panel.rows));
    panel.selected = panel.selected.min(listed.len().saturating_sub(1));
    if panel.has_selection || panel.followed.is_some() {
        if panel.selected < panel.offset {
            panel.offset = panel.selected;
        } else if panel.selected >= panel.offset + panel.rows {
            panel.offset = panel.selected + 1 - panel.rows;
        }
    }
    draw_gpu_process_rows(
        canvas,
        area,
        controls_y + 1,
        panel.rows,
        &listed,
        parents,
        GpuRowsContext {
            config: &app.config,
            sample: &app.sample,
            panel,
        },
    );
    if listed.is_empty() {
        let available = app.sample.gpus.iter().any(|gpu| {
            gpu.process_usage
                .as_ref()
                .is_some_and(|snapshot| snapshot.available)
        });
        canvas.text(
            area.x + 2,
            controls_y + 2,
            if available {
                "No matching GPU processes"
            } else {
                "GPU process data unavailable"
            },
            theme::LOW,
        );
    }
    let bottom = area.bottom() - 1;
    if listed.len() > panel.rows {
        let x = area.x + area.w - 2;
        let top = controls_y + 2;
        let end = bottom - 1;
        let span = end - top;
        let thumb = top
            + panel.offset * span.saturating_sub(1)
                / listed.len().saturating_sub(panel.rows).max(1);
        canvas.put_bold(x, controls_y + 1, '↑', theme::MAIN);
        canvas.put_bold(x, end, '↓', theme::MAIN);
        for y in top..end {
            canvas.put(x, y, if y == thumb { '█' } else { ' ' }, theme::MAIN);
        }
        panel.scrollbar = Some(ProcessScrollbar {
            x,
            up_y: controls_y + 1,
            down_y: end,
            track_top: top,
            track_bottom: end,
            thumb_y: thumb,
            total: listed.len(),
            visible: panel.rows,
        });
    }
    canvas.control_footer(
        area.x + 1,
        bottom,
        "↑ select ↓",
        &[0, 9],
        true,
        theme::PROC_BOX,
    );
    let location = format!(
        "{}/{}",
        if panel.has_selection {
            panel.selected + 1
        } else {
            0
        },
        listed.len()
    );
    let location_x = area.x + area.w.saturating_sub(location.len() + 3);
    let mut x = area.x + 13;
    for (label, action, hotkey) in [
        ("info ↵", ProcessControlAction::Info, 5),
        ("terminate", ProcessControlAction::Terminate, 0),
        ("kill", ProcessControlAction::Kill, 0),
        ("signals", ProcessControlAction::Signals, 0),
        ("Nice", ProcessControlAction::Nice, 0),
        ("Follow", ProcessControlAction::Follow, 0),
    ] {
        let length = units::display_width(label) + 2;
        if x + length > location_x {
            continue;
        }
        canvas.control_footer_state(
            x,
            bottom,
            label,
            &[hotkey],
            action != ProcessControlAction::Follow || panel.followed.is_some(),
            panel.has_selection,
            theme::PROC_BOX,
        );
        if panel.has_selection {
            panel.controls.push(ProcessControlHitbox {
                y: bottom,
                start: x + 1,
                end: x + length - 1,
                action,
            });
        }
        x += length;
    }
    canvas.footer(location_x, bottom, &location, theme::PROC_BOX);
    if panel.scrollbar.is_none() {
        panel.dragging = false;
    }
}
