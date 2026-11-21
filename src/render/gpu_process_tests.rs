use super::*;
use crate::process_gpu::{DeviceUsage, Snapshot, Usage};

fn process(pid: u32, parent: u32, bytes: Option<u64>, active: bool) -> ProcessSample {
    ProcessSample {
        pid,
        parent,
        start_time: Some(10),
        state: 'S',
        name: format!("worker{pid}"),
        command: format!("worker{pid} --run"),
        user: "test".into(),
        memory: 1_048_576,
        cpu: 7.0,
        gpu: Usage {
            dedicated_bytes: bytes,
            bytes,
            utilization: active.then_some(25),
            devices: if active {
                vec![DeviceUsage {
                    index: 0,
                    name: "test GPU".into(),
                    bytes,
                    utilization: Some(25),
                }]
            } else {
                vec![]
            },
        },
        ..ProcessSample::default()
    }
}

fn app() -> AppState {
    let mut config = Config::default();
    config.set_value("shown_boxes", "proc gpu_proc");
    let mut app = AppState::new(config);
    app.update(Sample {
        processes: vec![
            process(10, 0, Some(0), false),
            process(20, 10, Some(2_097_152), true),
            process(21, 10, None, true),
            process(22, 10, Some(0), false),
            process(30, 0, Some(4_194_304), true),
        ],
        gpus: vec![GpuSample {
            support: crate::gpu::GpuSupport {
                processes: true,
                memory_total: true,
                ..Default::default()
            },
            name: "test GPU".into(),
            memory_total: 16_777_216,
            process_usage: Some(Snapshot {
                available: true,
                complete: true,
                ..Snapshot::default()
            }),
            ..GpuSample::default()
        }],
        ..Sample::default()
    });
    app.sample.memory.total = 16_777_216;
    app
}

fn row(canvas: &Canvas, y: usize) -> String {
    canvas.cells[y * canvas.width..(y + 1) * canvas.width]
        .iter()
        .map(|cell| cell.ch)
        .collect()
}

fn render(app: &mut AppState) {
    Renderer::new().render(
        Size {
            cols: 120,
            rows: 40,
        },
        app,
    );
}

#[test]
fn panels_keep_filters_sorting_and_selection_independent() {
    let mut app = app();
    app.config.set_value("proc_sorting", "pid");
    render(&mut app);
    assert_eq!(app.visible_pids.len(), 5);
    assert_eq!(app.gpu_process_panel.visible, [30, 20, 21]);
    app.handle_key(Key::Tab);
    app.handle_key(Key::Down);
    app.handle_key(Key::Down);
    assert_eq!(app.selected_pid(), Some(20));
    app.handle_key(Key::Right);
    assert_eq!(app.config.value("gpu_proc_sorting"), Some("gpu usage"));
    assert_eq!(app.config.process_sort, ProcessSort::Pid);
    app.handle_key(Key::Char('f'));
    for ch in "worker20".chars() {
        app.handle_key(Key::Char(ch));
    }
    app.handle_key(Key::Enter);
    render(&mut app);
    assert_eq!(app.gpu_process_panel.visible, [20]);
    assert_eq!(app.visible_pids.len(), 5);
    assert!(app.config.process_filter.is_empty());
    app.handle_key(Key::Tab);
    app.handle_key(Key::Down);
    assert_eq!(app.selected_pid(), Some(30));
    assert_eq!(app.gpu_process_panel.visible, [20]);
}

#[test]
fn unsupported_backends_hide_gpu_process_ui_and_ignore_its_shortcut() {
    for name in [None, Some("AMD GPU"), Some("Intel GPU"), Some("NVIDIA GPU")] {
        let mut app = app();
        let mut sample = app.sample.clone();
        sample.gpus = name
            .into_iter()
            .map(|name| GpuSample {
                name: name.into(),
                ..Default::default()
            })
            .collect();
        app.update(sample);
        for requested in [false, true] {
            app.config.set_value(
                "shown_boxes",
                if requested { "proc gpu_proc" } else { "proc" },
            );
            let before = app.config.value("shown_boxes").unwrap().to_string();
            app.handle_key(Key::Char('v'));
            assert_eq!(app.config.value("shown_boxes"), Some(before.as_str()));
            let size = Size {
                cols: 120,
                rows: 40,
            };
            assert_eq!(
                minimum_size(&app.config, &app.sample.gpus),
                Size { cols: 44, rows: 16 }
            );
            let frame = Renderer::new().render(size, &mut app);
            assert!(!frame.contains("gpu view") && !frame.contains("gpu proc"));
            assert!(app.gpu_process_panel.area.is_none());
            assert!(
                !app.process_control_hitboxes
                    .iter()
                    .any(|hit| hit.action == ProcessControlAction::GpuProcesses)
            );
            assert_eq!(app.process_area.unwrap().h, 40);
            app.detailed_pid = Some(20);
            let frame = Renderer::new().render(size, &mut app);
            assert!(!frame.contains("GPU:"));
            app.detailed_pid = None;
        }
    }
}

#[test]
fn supported_idle_gpu_keeps_controls_and_backend_loss_clears_focus() {
    let mut app = app();
    let supported = app.sample.clone();
    let mut idle = supported.clone();
    idle.processes.clear();
    app.update(idle);
    render(&mut app);
    assert!(app.gpu_process_panel.area.is_some());
    assert!(app.gpu_process_panel.visible.is_empty());
    assert!(
        app.process_control_hitboxes
            .iter()
            .any(|hit| hit.action == ProcessControlAction::GpuProcesses)
    );
    app.gpu_process_focus = true;
    app.handle_key(Key::Char('f'));
    assert!(app.gpu_process_panel.editing_filter);
    app.update(Sample::default());
    render(&mut app);
    assert!(!app.gpu_process_focus);
    assert!(!app.gpu_process_panel.editing_filter);
    assert!(app.gpu_process_panel.area.is_none());
    assert!(app.config.gpu_processes_shown());
    app.update(supported);
    render(&mut app);
    assert!(app.gpu_process_panel.area.is_some());
    assert_eq!(app.gpu_process_panel.visible.len(), 3);
}

#[test]
fn gpu_process_options_follow_capability_without_changing_saved_settings() {
    let mut app = app();
    assert!(app.option_keys(5).contains(&"gpu_proc_sorting"));
    app.config.set_value("gpu_proc_filter", "saved-filter");
    app.options_category = 5;
    let index = app
        .option_keys(5)
        .iter()
        .position(|option| *option == "keep_dead_proc_usage")
        .unwrap();
    app.options_page = index / app.options_per_page();
    app.options_selected = index % app.options_per_page();
    app.update(Sample::default());
    assert_eq!(app.current_option(), Some("keep_dead_proc_usage"));
    let options = app.option_keys(5);
    assert!(!options.iter().any(|option| option.starts_with("gpu_proc_")));
    app.options_category = 5;
    app.options_selected = 0;
    app.options_page = 0;
    for option in &options {
        assert_eq!(app.current_option(), Some(*option));
        app.move_option(1);
    }
    assert_eq!(app.current_option(), Some(options[0]));
    assert_eq!(app.config.value("gpu_proc_filter"), Some("saved-filter"));
}

#[test]
fn gpu_panel_toggle_preserves_regular_controls_and_rows() {
    let mut app = app();
    render(&mut app);
    app.handle_key(Key::Char('v'));
    assert!(!app.config.gpu_processes_shown());
    assert!(app.take_collection_request());
    render(&mut app);
    assert!(app.gpu_process_panel.area.is_none());
    assert_eq!(app.visible_pids.len(), 5);
    let hit = *app
        .process_control_hitboxes
        .iter()
        .find(|hit| hit.action == ProcessControlAction::GpuProcesses)
        .unwrap();
    app.handle_mouse(0, hit.start, hit.y, true);
    assert!(app.config.gpu_processes_shown());
    render(&mut app);
    assert!(app.gpu_process_panel.area.is_some());
    assert!(app.process_area.is_some());
    assert_eq!(app.visible_pids.len(), 5);
}

#[test]
fn gpu_panel_activation_from_presets_and_options_requests_fresh_data() {
    for key in [Key::Char('p'), Key::Char('P')] {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        app.config
            .set_value("presets", "proc:0:default,gpu_proc:0:default");
        app.config.preset = Some(0);
        app.handle_key(key);
        assert!(app.config.gpu_processes_shown());
        assert!(app.take_collection_request(), "preset key {key:?}");
    }
    for (option, value) in [("shown_boxes", "proc gpu_proc"), ("shown_gpus", "nvidia")] {
        let mut app = app();
        app.config.set_value("shown_boxes", "proc");
        app.config.set_value("background_update", "false");
        app.overlay = Overlay::Options;
        app.editing_option = Some(option);
        app.options_buffer = value.into();
        app.handle_key(Key::Enter);
        assert!(app.editing_option.is_none());
        assert!(!app.take_collection_request());
        app.handle_key(Key::Escape);
        assert!(app.take_collection_request(), "option {option}");
        assert!(!app.take_collection_request());
    }
}

#[test]
fn gpu_panel_uses_the_process_graph_symbol_override() {
    let size = Size {
        cols: 120,
        rows: 20,
    };
    for symbol in ["tty", "block", "braille"] {
        let mut app = app();
        app.config.set_value("shown_boxes", "gpu_proc");
        app.config.set_value("graph_symbol", symbol);
        app.config.set_value("graph_symbol_proc", "default");
        let expected = Renderer::new().render(size, &mut app);
        app.config.set_value(
            "graph_symbol",
            if symbol == "tty" { "braille" } else { "tty" },
        );
        app.config.set_value("graph_symbol_proc", symbol);
        assert!(
            Renderer::new().render(size, &mut app) == expected,
            "symbol {symbol}"
        );
    }
}

#[test]
fn gpu_vram_percentage_uses_dedicated_bytes_and_requires_all_device_capacities() {
    let mut app = app();
    let process = app
        .gpu_process_panel
        .processes
        .iter_mut()
        .find(|p| p.pid == 20)
        .unwrap();
    process.gpu.dedicated_bytes = Some(1_048_576);
    app.sample.gpus[0].support.memory_total = true;
    let mut second = app.sample.gpus[0].clone();
    second.memory_total = 0;
    second.support.memory_total = false;
    app.sample.gpus.push(second);
    for (known, expected) in [(false, "—"), (true, "3%")] {
        app.sample.gpus[1].support.memory_total = known;
        if known {
            app.sample.gpus[1].memory_total = 16_777_216;
        }
        let mut canvas = Canvas::new(120, 20);
        let processes = app.gpu_process_panel.listed_processes(&app.config);
        draw_gpu_process_panel(&mut canvas, Rect::new(0, 0, 120, 20), &mut app, processes);
        let x = row(&canvas, 1).find("VRAM%").unwrap();
        let y = app
            .gpu_process_panel
            .hits
            .iter()
            .find(|hit| hit.pid == 20)
            .unwrap()
            .y;
        let value: String = canvas.cells[y * 120 + x..y * 120 + x + 5]
            .iter()
            .map(|cell| cell.ch)
            .collect();
        assert_eq!(value.trim(), expected);
    }
}

#[test]
fn gpu_panel_height_fits_filtered_rows_and_returns_space_to_regular_list() {
    let mut app = app();
    let size = Size {
        cols: 120,
        rows: 80,
    };
    let mut renderer = Renderer::new();
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 6);
    assert_eq!(app.process_area.unwrap().h, 74);
    app.config.set_value("gpu_proc_filter", "worker20");
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 4);
    assert_eq!(app.process_area.unwrap().h, 76);
    app.config
        .set_value("gpu_proc_filter", "no-matching-process");
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 4);
    app.config.set_value("gpu_proc_filter", "");
    app.config.set_value("gpu_proc_tree", "true");
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 7);
    app.gpu_process_panel.collapsed.insert(10);
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 5);
}

#[test]
fn gpu_height_cap_and_details_resize_without_losing_rows() {
    let mut app = app();
    let size = Size {
        cols: 120,
        rows: 80,
    };
    let mut renderer = Renderer::new();
    let mut next = app.sample.clone();
    next.processes = (1..=40)
        .map(|pid| process(pid, 0, Some(1000), true))
        .collect();
    app.update(next);
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 20);
    assert_eq!(app.gpu_process_panel.visible.len(), 40);
    app.gpu_process_focus = true;
    app.handle_key(Key::End);
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.hits.last().unwrap().index, 39);
    app.config.set_value("gpu_proc_filter", "worker40");
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 4);
    app.handle_key(Key::Enter);
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 14);
    assert_eq!(app.gpu_process_panel.hits.len(), 1);
    assert!(app.detailed_pid.is_none());
    app.handle_key(Key::Enter);
    renderer.render(size, &mut app);
    assert_eq!(app.gpu_process_panel.area.unwrap().h, 4);
}

#[test]
fn gpu_rows_keep_native_column_order_and_telemetry_at_small_widths() {
    for width in [44, 55, 75, 85, 100, 160] {
        for tree in [false, true] {
            let mut app = app();
            app.config.set_value("gpu_proc_tree", tree.to_string());
            app.gpu_process_panel.processes[1].user = "界e\u{301}".into();
            let mut canvas = Canvas::new(width, 20);
            let processes = app.gpu_process_panel.listed_processes(&app.config);
            draw_gpu_process_panel(&mut canvas, Rect::new(0, 0, width, 20), &mut app, processes);
            let header: Vec<_> = canvas.cells[width..2 * width]
                .iter()
                .map(|cell| cell.ch)
                .collect();
            let memory_x = header
                .windows(6)
                .position(|word| word == ['G', 'P', 'U', 'M', 'e', 'm'])
                .unwrap();
            let cpu_x = header
                .windows(4)
                .position(|word| word == ['C', 'p', 'u', '%'])
                .unwrap();
            let hit = app
                .gpu_process_panel
                .hits
                .iter()
                .find(|hit| hit.pid == 20)
                .unwrap();
            let cells = &canvas.cells[hit.y * width..(hit.y + 1) * width];
            let value: String = cells[memory_x..memory_x + 6]
                .iter()
                .map(|cell| cell.ch)
                .collect();
            assert_eq!(value.trim(), "2.0M");
            let cpu: String = cells[cpu_x..cpu_x + 4].iter().map(|cell| cell.ch).collect();
            assert_eq!(cpu.trim(), "7.0");
            assert!(row(&canvas, 1).contains("MemB"));
            assert!(row(&canvas, 19).contains("select"));
            if !tree && width >= 85 {
                let header = row(&canvas, 1);
                let order: Vec<_> = [
                    "Pid:", "Program:", "Command:", "User:", "Gpu%", "GPUMem", "MemB", "Cpu%",
                ]
                .iter()
                .map(|title| header.find(title).unwrap())
                .collect();
                assert!(order.windows(2).all(|pair| pair[0] < pair[1]));
            }
        }
    }
}

#[test]
fn gpu_filter_tree_and_sort_preserve_unknown_readings() {
    for sort in [ProcessSort::GpuMemory, ProcessSort::GpuUsage] {
        for reversed in [false, true] {
            let mut app = app();
            for process in &mut app.gpu_process_panel.processes {
                process.gpu.utilization = process.gpu.bytes.map(|bytes| (bytes / 104_857) as u32);
            }
            app.config.set_value("gpu_proc_sorting", sort.label());
            app.config
                .set_value("gpu_proc_reversed", reversed.to_string());
            render(&mut app);
            assert_eq!(
                app.gpu_process_panel.visible,
                if reversed {
                    vec![20, 30, 21]
                } else {
                    vec![30, 20, 21]
                }
            );
        }
    }
    let mut app = app();
    app.config.set_value("gpu_proc_tree", "true");
    render(&mut app);
    assert_eq!(
        app.gpu_process_panel
            .visible
            .iter()
            .copied()
            .collect::<HashSet<_>>(),
        HashSet::from([10, 20, 21, 30])
    );
    app.config.set_value("gpu_proc_filter", "worker20");
    render(&mut app);
    assert_eq!(app.gpu_process_panel.visible, [20]);
}

#[test]
fn gpu_actions_and_details_stay_in_the_gpu_panel() {
    let mut app = app();
    render(&mut app);
    let hit = *app
        .gpu_process_panel
        .hits
        .iter()
        .find(|hit| hit.pid == 20)
        .unwrap();
    app.handle_mouse(0, 60, hit.y, true);
    assert_eq!(app.selected_pid(), Some(20));
    app.handle_key(Key::Char('t'));
    assert_eq!(
        app.overlay,
        Overlay::Signal {
            pid: 20,
            signal: 15
        }
    );
    app.handle_key(Key::Escape);
    app.handle_key(Key::Enter);
    assert_eq!(app.detailed_pid, None);
    assert_eq!(app.gpu_process_panel.detail.as_ref().unwrap().pid, 20);
    assert!(app.gpu_process_focus);
    assert!(app.take_collection_request());
    assert_eq!(app.detailed_pids(), [None, Some(20)]);
    app.handle_key(Key::Enter);
    assert!(app.gpu_process_panel.detail.is_none());
    app.config.set_value("shown_boxes", "gpu_proc");
    render(&mut app);
    app.handle_key(Key::Home);
    app.handle_key(Key::Enter);
    assert!(app.gpu_process_panel.detail.is_some());
    assert!(!app.config.shown[3]);
}

#[test]
fn paused_lists_are_independent_and_pid_reuse_clears_gpu_selection() {
    let mut app = app();
    render(&mut app);
    app.config.pause_processes = true;
    let mut next = app.sample.clone();
    next.processes[1].gpu.bytes = Some(5_000_000);
    app.update(next.clone());
    assert_eq!(app.sample.processes[1].gpu.bytes, Some(2_097_152));
    assert_eq!(
        app.gpu_process_panel.processes[1].gpu.bytes,
        Some(5_000_000)
    );
    app.handle_key(Key::Tab);
    app.handle_key(Key::Char('u'));
    next.processes[1].gpu.bytes = Some(6_000_000);
    app.config.pause_processes = false;
    app.update(next.clone());
    assert_eq!(app.sample.processes[1].gpu.bytes, Some(6_000_000));
    assert_eq!(
        app.gpu_process_panel.processes[1].gpu.bytes,
        Some(5_000_000)
    );
    render(&mut app);
    app.handle_key(Key::Down);
    app.handle_key(Key::Down);
    let pid = app.selected_pid().unwrap();
    next.processes
        .iter_mut()
        .find(|process| process.pid == pid)
        .unwrap()
        .start_time = Some(11);
    app.update(next);
    assert!(!app.gpu_process_panel.has_selection);
    render(&mut app);
    let index = app
        .gpu_process_panel
        .visible
        .iter()
        .position(|entry| *entry == pid)
        .unwrap();
    app.gpu_process_panel.selected = index;
    app.gpu_process_panel.has_selection = true;
    app.handle_key(Key::Char('t'));
    assert_eq!(
        app.overlay,
        Overlay::None,
        "a frozen row must not target a reused PID"
    );
}

#[test]
fn gpu_panel_layout_reserves_space_and_recovers_from_tiny_sizes() {
    for boxes in [
        "gpu_proc",
        "proc gpu_proc",
        "cpu mem net proc gpu_proc",
        "cpu gpu0 mem net proc gpu_proc",
    ] {
        let mut app = app();
        app.config.set_value("shown_boxes", boxes);
        let minimum = minimum_size(&app.config, &app.sample.gpus);
        for size in [
            minimum,
            Size {
                cols: 160,
                rows: 60,
            },
        ] {
            Renderer::new().render(size, &mut app);
            let gpu = app.gpu_process_panel.area.unwrap();
            assert!(gpu.h >= 4 && gpu.w >= 44);
            for other in [
                app.process_area,
                app.cpu_area,
                app.memory_area,
                app.network_area,
            ]
            .into_iter()
            .flatten()
            {
                assert!(gpu.bottom() <= other.y || other.bottom() <= gpu.y);
            }
        }
        let mut renderer = Renderer::new();
        renderer.render(Size { cols: 2, rows: 2 }, &mut app);
        assert!(app.gpu_process_panel.area.is_none());
        renderer.render(minimum, &mut app);
        assert!(app.gpu_process_panel.area.is_some());
    }
}

#[test]
fn gpu_navigation_and_tree_shortcuts_stay_in_the_focused_panel() {
    let mut app = app();
    app.config.set_value("vim_keys", "true");
    app.config.set_value("proc_tree", "true");
    app.config.set_value("gpu_proc_tree", "true");
    render(&mut app);
    app.process_selected = true;
    app.gpu_process_focus = true;
    app.handle_key(Key::Char('G'));
    assert!(app.gpu_process_panel.has_selection);
    assert_eq!(
        app.gpu_process_panel.selected + 1,
        app.gpu_process_panel.visible.len()
    );
    app.handle_key(Key::Char('F'));
    app.handle_key(Key::Char('g'));
    assert_eq!(app.gpu_process_panel.selected, 0);
    assert_eq!(app.gpu_process_panel.followed, None);
    let parent = app
        .gpu_process_panel
        .visible
        .iter()
        .position(|pid| *pid == 10)
        .unwrap();
    app.gpu_process_panel.selected = parent;
    app.handle_key(Key::Char('C'));
    assert!(app.gpu_process_panel.collapsed.contains(&20));
    assert!(app.collapsed_processes.is_empty());
    app.handle_key(Key::Char('E'));
    assert!(app.collapsed_processes.is_empty());
    assert_eq!(app.selected_process, 0);
}

#[test]
fn dragging_regular_scrollbar_across_gpu_panel_keeps_regular_focus() {
    let mut app = app();
    let mut sample = app.sample.clone();
    sample.processes = (1..=100)
        .map(|pid| process(pid, 0, Some(u64::from(pid)), true))
        .collect();
    app.update(sample);
    render(&mut app);
    app.handle_key(Key::End);
    render(&mut app);
    let scrollbar = app.process_scrollbar.unwrap();
    app.handle_mouse(0, scrollbar.x, scrollbar.thumb_y, true);
    assert!(app.dragging_process_scrollbar);
    assert!(app.selected_process > 0);
    render(&mut app);
    let gpu = app.gpu_process_panel.area.unwrap();
    app.handle_mouse(32, scrollbar.x, gpu.y + 2, true);
    assert!(!app.gpu_process_focus);
    assert_eq!(app.selected_process, 0);
    app.handle_mouse(0, scrollbar.x, gpu.y + 2, false);
    assert!(!app.dragging_process_scrollbar);
}

#[test]
fn gpu_arrow_selection_matches_regular_process_navigation() {
    let mut app = app();
    render(&mut app);
    app.gpu_process_focus = true;
    app.handle_key(Key::Up);
    assert!(!app.gpu_process_panel.has_selection);
    app.handle_key(Key::Down);
    assert!(app.gpu_process_panel.has_selection);
    assert_eq!(app.gpu_process_panel.selected, 0);
    app.handle_key(Key::Down);
    assert_eq!(app.gpu_process_panel.selected, 1);
    app.handle_key(Key::Up);
    assert_eq!(app.gpu_process_panel.selected, 0);
    assert!(app.gpu_process_panel.has_selection);
    app.handle_key(Key::Up);
    assert!(!app.gpu_process_panel.has_selection);
}

#[test]
fn hiding_or_shrinking_gpu_panel_cancels_its_scrollbar_drag() {
    for shrink in [false, true] {
        let mut app = app();
        let mut sample = app.sample.clone();
        sample.processes = (1..=100)
            .map(|pid| process(pid, 0, Some(u64::from(pid)), true))
            .collect();
        app.update(sample);
        render(&mut app);
        let gpu = app.gpu_process_panel.area.unwrap();
        app.handle_mouse(0, gpu.x + gpu.w - 2, gpu.y + 2, true);
        assert!(app.gpu_process_focus);
        if shrink {
            Renderer::new().render(Size { cols: 2, rows: 2 }, &mut app);
        } else {
            app.handle_key(Key::Char('v'));
            render(&mut app);
            app.handle_key(Key::Char('v'));
        }
        render(&mut app);
        let hit = app.process_hitboxes[0];
        app.handle_mouse(0, app.process_area.unwrap().x + 2, hit.y, true);
        assert!(!app.gpu_process_focus, "shrink={shrink}");
        assert!(app.process_selected);
    }
}

#[test]
fn gpu_details_remain_live_when_regular_list_is_paused() {
    let mut app = app();
    app.config.pause_processes = true;
    let mut next = app.sample.clone();
    next.processes.push(process(40, 0, Some(8_388_608), true));
    app.update(next.clone());
    render(&mut app);
    app.gpu_process_focus = true;
    app.handle_key(Key::Home);
    assert_eq!(app.selected_pid(), Some(40));
    app.handle_key(Key::Enter);
    app.update(next.clone());
    render(&mut app);
    assert_eq!(app.gpu_process_panel.detail.as_ref().unwrap().state, 'S');
    assert!(!app.gpu_process_panel.detail_cpu_history.is_empty());
    app.handle_key(Key::Char('t'));
    assert_eq!(
        app.overlay,
        Overlay::Signal {
            pid: 40,
            signal: 15
        }
    );
    app.handle_key(Key::Escape);
    next.processes.retain(|process| process.pid != 40);
    app.update(next);
    render(&mut app);
    assert_eq!(app.gpu_process_panel.detail.as_ref().unwrap().state, 'X');
    app.handle_key(Key::Char('t'));
    assert_eq!(app.overlay, Overlay::None);
}

#[test]
fn reused_gpu_pid_does_not_inherit_history_or_collapse_state() {
    let mut app = app();
    let mut next = app.sample.clone();
    app.gpu_process_panel.collapsed.insert(20);
    next.processes[1].start_time = Some(11);
    next.processes[1].cpu = 12.0;
    app.update(next);
    assert!(!app.gpu_process_panel.collapsed.contains(&20));
    assert_eq!(
        app.gpu_process_panel.cpu_histories[&20],
        VecDeque::from([12.0])
    );
}

#[test]
fn normal_selection_is_invalidated_on_pid_reuse_while_gpu_has_focus() {
    let mut app = app();
    render(&mut app);
    app.process_selected = true;
    let pid = app.selected_pid().unwrap();
    app.gpu_process_focus = true;
    let mut next = app.sample.clone();
    next.processes
        .iter_mut()
        .find(|process| process.pid == pid)
        .unwrap()
        .start_time = Some(11);
    app.update(next);
    assert!(!app.process_selected);
}

#[test]
fn two_detail_panels_keep_targets_and_layout_independent() {
    let mut app = app();
    render(&mut app);
    app.process_selected = true;
    app.handle_key(Key::Enter);
    let regular = app.detailed_pid.unwrap();
    app.gpu_process_focus = true;
    app.handle_key(Key::Home);
    let gpu = app.selected_pid().unwrap();
    app.handle_key(Key::Enter);
    assert_eq!(app.detailed_pids(), [Some(regular), Some(gpu)]);
    let next = app.sample.clone();
    app.update(next);
    for (width, height) in [(44, 8), (55, 12), (85, 14), (160, 20)] {
        let mut canvas = Canvas::new(width, height);
        let processes = app.gpu_process_panel.listed_processes(&app.config);
        draw_gpu_process_panel(
            &mut canvas,
            Rect::new(0, 0, width, height),
            &mut app,
            processes,
        );
        let header_y = if height >= 14 { 8 } else { 4 };
        assert!(row(&canvas, header_y).contains("gpu proc"));
        assert!(row(&canvas, header_y + 1).contains("GPUMem"));
        assert!(
            app.gpu_process_panel
                .hits
                .iter()
                .all(|hit| hit.y > header_y + 1 && hit.y < height - 1)
        );
        assert_eq!(app.detailed_pid, Some(regular));
    }
    render(&mut app);
    let hide = *app
        .gpu_process_panel
        .controls
        .iter()
        .find(|hit| {
            hit.action == ProcessControlAction::Info
                && hit.y == app.gpu_process_panel.area.unwrap().y
        })
        .unwrap();
    app.handle_mouse(0, hide.start, hide.y, true);
    assert!(app.gpu_process_panel.detail.is_none());
    assert_eq!(app.detailed_pid, Some(regular));
}
