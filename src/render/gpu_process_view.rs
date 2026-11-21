use super::gpu_process_panel::GpuProcessPanel;
use super::*;

pub(super) struct GpuRowsContext<'a> {
    pub config: &'a Config,
    pub sample: &'a Sample,
    pub panel: &'a mut GpuProcessPanel,
}

pub(super) fn draw_gpu_process_rows(
    canvas: &mut Canvas,
    area: Rect,
    header_y: usize,
    rows: usize,
    listed: &[(&ProcessSample, usize)],
    parent_pids: &HashSet<u32>,
    context: GpuRowsContext<'_>,
) {
    let GpuRowsContext {
        config,
        sample,
        panel,
    } = context;
    let threads = area.w >= 75;
    let devices = area.w >= 85;
    let show_memory_percent = area.w >= 110;
    let graphs = area.w >= 75 && config.bool_value("proc_cpu_graphs").unwrap_or(true);
    let user_width = if area.w < 75 {
        5
    } else {
        listed
            .iter()
            .skip(panel.offset)
            .take(rows)
            .map(|(process, _)| units::display_width(&process.user))
            .max()
            .unwrap_or(0)
            .clamp(5, 10)
    };
    let mut columns = Vec::new();
    if threads {
        columns.push(("Threads:", 4));
    }
    columns.push(("User:", user_width));
    if devices {
        columns.push(("Dev", 3));
    }
    columns.extend([("Gpu%", 4), ("GPUMem", 6)]);
    if show_memory_percent {
        columns.push(("VRAM%", 5));
    }
    columns.extend([("MemB", 5), ("Cpu%", if graphs { 10 } else { 4 })]);
    let metrics_width: usize = columns.iter().map(|(_, width)| width + 1).sum();
    let left_width = area.w.saturating_sub(metrics_width + 3);
    let program_width = (if area.w >= 90 {
        16
    } else if area.w > 55 {
        8
    } else {
        left_width.saturating_sub(10)
    })
    .min(left_width.saturating_sub(10))
    .max(1);
    let command_width = left_width.saturating_sub(9 + program_width + 2);
    let mut leading = if panel.tree {
        vec![("Tree:", left_width.saturating_sub(1))]
    } else {
        vec![("Pid:", 8), ("Program:", program_width)]
    };
    if !panel.tree && command_width > 0 {
        leading.push(("Command:", command_width));
    }
    let mut x = area.x + 1;
    for (index, (title, width)) in leading.iter().enumerate() {
        let title_width = if *title == "Command:" && threads {
            width.saturating_sub(4)
        } else {
            *width
        };
        let title = clip_text(title, title_width);
        let title = if index == 0 && !panel.tree {
            format!("{title:>width$}")
        } else {
            title
        };
        canvas.text_bold(x, header_y, &title, theme::TITLE);
        x += width + 1;
    }
    x = area.x + 1 + left_width;
    for (title, width) in &columns {
        let title_x = if *title == "Threads:" {
            x.saturating_sub(4)
        } else {
            x
        };
        canvas.text_bold(title_x, header_y, &format!("{title:>width$}"), theme::TITLE);
        x += width + 1;
    }
    let colors = config.bool_value("proc_colors").unwrap_or(true);
    let gradient = config.bool_value("proc_gradient").unwrap_or(true);
    let capacity = sample
        .gpus
        .iter()
        .filter(|gpu| gpu.process_usage.is_some())
        .try_fold(0_u64, |total, gpu| {
            gpu.support
                .memory_total
                .then(|| total.checked_add(gpu.memory_total))
                .flatten()
        })
        .filter(|capacity| *capacity > 0);
    for (index, (process, depth)) in listed.iter().enumerate().skip(panel.offset).take(rows) {
        let y = header_y + 1 + index - panel.offset;
        let selected = panel.has_selection && index == panel.selected;
        let followed = panel.followed == Some(process.pid);
        let emphasis = if followed {
            Some(theme::FOLLOWED)
        } else if selected {
            Some(theme::SELECTED)
        } else {
            None
        };
        if let Some(style) = emphasis {
            canvas.fill(area.x + 1, y, area.w - 2, ' ', style);
        }
        let distance = index.abs_diff(panel.selected);
        let general = if gradient {
            theme::Style::Proc((distance * 100 / rows.max(1)).min(100) as u8)
        } else {
            theme::MAIN
        };
        let metric = |value| process_metric_style(value, distance, rows, colors, gradient);
        let gpu_style = process
            .gpu
            .utilization
            .map_or(theme::LOW, |value| metric(f64::from(value)));
        let memory_percent = process
            .gpu
            .dedicated_bytes
            .zip(capacity)
            .map(|(bytes, capacity)| ratio(bytes, capacity));
        let memory_style = memory_percent.map_or(theme::LOW, metric);
        let command = sanitize_control(if process.command.is_empty() {
            &process.name
        } else {
            &process.command
        });
        let name = sanitize_control(&process.name);
        let mut names = if panel.tree {
            let prefix = process_tree_prefix(listed, index, parent_pids, &panel.collapsed);
            vec![(format!("{prefix}{} {command}", process.pid), general)]
        } else {
            vec![(format!("{:>8}", process.pid), general), (name, gpu_style)]
        };
        if !panel.tree && command_width > 0 {
            names.push((command, general));
        }
        let mut values = Vec::new();
        if threads {
            values.push((
                process_threads(process.threads),
                metric(process.threads as f64 / 3.0),
            ));
        }
        values.push((clip_text_with_plus(&process.user, user_width), general));
        if devices {
            let devices = process
                .gpu
                .devices
                .iter()
                .map(|device| device.index.to_string())
                .collect::<Vec<_>>()
                .join(",");
            values.push((
                if devices.is_empty() {
                    "—".into()
                } else {
                    devices
                },
                general,
            ));
        }
        values.push((
            process
                .gpu
                .utilization
                .map(|value| value.to_string())
                .unwrap_or_else(|| "—".into()),
            gpu_style,
        ));
        values.push((
            process
                .gpu
                .bytes
                .map(|value| units::bytes_short(value, config.base_10_sizes))
                .unwrap_or_else(|| "—".into()),
            memory_style,
        ));
        if show_memory_percent {
            values.push((
                memory_percent
                    .map(|value| format!("{value:.0}%"))
                    .unwrap_or_else(|| "—".into()),
                memory_style,
            ));
        }
        values.push((
            units::bytes_short(process.memory, config.base_10_sizes),
            metric(ratio(process.memory, sample.memory.total)),
        ));
        let cpu = if process.cpu < 100.0 {
            format!("{:.1}", process.cpu)
        } else {
            format!("{:.0}", process.cpu)
        };
        values.push((cpu, metric(process.cpu)));
        for (columns, values, start, right_align) in [
            (&leading, names, area.x + 1, false),
            (&columns, values, area.x + 1 + left_width, true),
        ] {
            let mut x = start;
            for ((title, width), (value, style)) in columns.iter().zip(values) {
                let value = if right_align && *title != "User:" {
                    clip_text_with_plus(&value, *width)
                } else {
                    clip_text(&value, *width)
                };
                let value = if right_align && *title != "User:" {
                    format!(
                        "{}{value}",
                        " ".repeat(width.saturating_sub(units::display_width(&value)))
                    )
                } else {
                    value
                };
                if let Some(style) = emphasis {
                    canvas.text_bold(x, y, &value, style);
                } else if colors {
                    canvas.text(x, y, &value, style);
                } else {
                    canvas.text_bold(x, y, &value, theme::MAIN);
                }
                x += width + 1;
            }
        }
        if graphs && let Some(history) = panel.cpu_histories.get(&process.pid) {
            let graph_x = area.x + 1 + left_width + metrics_width - 11;
            if let Some(style) = emphasis {
                for offset in 0..5 {
                    canvas.put_bold(graph_x + offset, y, graph_background_char(canvas), style);
                }
                let mut graph = Canvas::new(5, 1);
                graph.graph_symbol = canvas.graph_symbol;
                graph.tty = canvas.tty;
                draw_graph_options(
                    &mut graph,
                    Rect::new(0, 0, 5, 1),
                    history,
                    100.0,
                    metric(process.cpu),
                    false,
                    false,
                );
                for (offset, cell) in graph.cells.into_iter().enumerate() {
                    if cell.ch != ' ' {
                        canvas.put_bold(graph_x + offset, y, cell.ch, style);
                    }
                }
            } else {
                draw_graph_background(canvas, Rect::new(graph_x, y, 5, 1));
                draw_graph_options(
                    canvas,
                    Rect::new(graph_x, y, 5, 1),
                    history,
                    100.0,
                    metric(process.cpu),
                    false,
                    false,
                );
            }
        }
        let toggle_x = (panel.tree && parent_pids.contains(&process.pid))
            .then(|| area.x + 1 + depth * 3)
            .filter(|x| *x + 3 <= area.x + 1 + left_width)
            .map(|x| (x, x + 3));
        panel.hits.push(ProcessHitbox {
            y,
            index,
            pid: process.pid,
            toggle_x,
        });
    }
}
