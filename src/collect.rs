use std::collections::{HashMap, HashSet};
use std::ffi::CString;
use std::fs;
use std::os::raw::{c_char, c_int, c_uint, c_ulong};
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::Instant;

use crate::config::Config;
use crate::gpu::{GpuCollector, GpuSample};

#[cfg(target_os = "macos")]
mod macos;

#[derive(Debug, Clone, Default)]
pub struct CpuSample {
    pub total: f64,
    pub fields: HashMap<String, f64>,
    pub cores: Vec<f64>,
    pub load: [f64; 3],
    pub frequency: String,
    pub temperature: Option<f64>,
    pub temperature_max: f64,
    pub core_temperatures: Vec<Option<f64>>,
    pub name: String,
    pub uptime: u64,
    pub battery: Option<BatterySample>,
    pub watts: Option<f64>,
    pub container_engine: Option<String>,
    pub active_cpus: Option<HashSet<usize>>,
    pub available_batteries: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct BatterySample {
    pub percent: u8,
    pub status: String,
    pub watts: Option<f64>,
    pub seconds: Option<u64>,
}

#[derive(Debug, Clone, Default)]
pub struct MemorySample {
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub available: u64,
    pub cached: u64,
    pub swap_total: u64,
    pub swap_used: u64,
    pub disks: Vec<DiskSample>,
}

#[derive(Debug, Clone, Default)]
pub struct DiskSample {
    pub mount: String,
    pub total: u64,
    pub used: u64,
    pub free: u64,
    pub io_supported: bool,
    pub read_per_second: u64,
    pub write_per_second: u64,
    pub io_activity: f64,
}

#[derive(Debug, Clone, Default)]
pub struct NetworkSample {
    pub interfaces: Vec<String>,
    pub selected: String,
    pub download_per_second: u64,
    pub upload_per_second: u64,
    pub downloaded: u64,
    pub uploaded: u64,
    pub ipv4: Option<String>,
    pub ipv6: Option<String>,
    pub connected: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub command: String,
    pub user: String,
    pub state: char,
    pub threads: u32,
    pub memory: u64,
    pub cpu: f64,
    pub cumulative_cpu: f64,
    pub nice: i32,
    pub kernel_thread: bool,
    pub elapsed_seconds: u64,
    pub read_bytes: u64,
    pub write_bytes: u64,
}

#[derive(Debug, Clone, Default)]
pub struct Sample {
    pub cpu: CpuSample,
    pub memory: MemorySample,
    pub network: NetworkSample,
    pub processes: Vec<ProcessSample>,
    pub process_count: usize,
    pub gpus: Vec<GpuSample>,
    pub collection_times_us: [u64; 6],
}

#[derive(Debug, Clone, Copy, Default)]
struct CpuTicks {
    busy: u64,
    total: u64,
    fields: [u64; 10],
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct DiskCounters {
    sectors_read: u64,
    sectors_written: u64,
    io_milliseconds: u64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct NetworkCounters {
    receive_last: u64,
    transmit_last: u64,
    receive_rollover: u64,
    transmit_rollover: u64,
}

pub struct Collector {
    previous_cpu: Vec<CpuTicks>,
    previous_processes: HashMap<u32, u64>,
    previous_network: HashMap<String, NetworkCounters>,
    previous_disks: HashMap<String, DiskCounters>,
    last_sample: Instant,
    cpu_name: String,
    users: HashMap<u32, String>,
    gpus: GpuCollector,
    rapl_previous: Option<(u64, Instant)>,
    container_engine: Option<String>,
}

impl Collector {
    pub fn new(config: &Config) -> Result<Self, String> {
        Ok(Self {
            previous_cpu: Vec::new(),
            previous_processes: HashMap::new(),
            previous_network: HashMap::new(),
            previous_disks: HashMap::new(),
            last_sample: Instant::now(),
            cpu_name: if cfg!(target_os = "macos") {
                #[cfg(target_os = "macos")]
                {
                    macos::read_cpu_name()
                }
                #[cfg(not(target_os = "macos"))]
                {
                    String::new()
                }
            } else {
                read_cpu_name()
            },
            users: read_users(),
            gpus: GpuCollector::new(config),
            rapl_previous: None,
            container_engine: detect_container(),
        })
    }

    pub fn collect(
        &mut self,
        config: &Config,
        detailed_pid: Option<u32>,
    ) -> Result<Sample, String> {
        let collection_started = Instant::now();
        let elapsed = self.last_sample.elapsed().as_secs_f64().max(0.001);
        self.last_sample = Instant::now();
        let mut collection_times_us = [0; 6];
        let started = Instant::now();
        let (cpu, total_delta) = self.collect_cpu(config)?;
        collection_times_us[0] = elapsed_us(started);
        let started = Instant::now();
        let memory = collect_memory(config, &mut self.previous_disks, elapsed)?;
        collection_times_us[1] = elapsed_us(started);
        let started = Instant::now();
        let network = self.collect_network(config, elapsed)?;
        collection_times_us[2] = elapsed_us(started);
        let started = Instant::now();
        let processes = self.collect_processes(
            total_delta,
            cpu.cores.len().max(1),
            memory.total,
            config,
            detailed_pid,
        )?;
        collection_times_us[3] = elapsed_us(started);
        let process_count = processes.len();
        let started = Instant::now();
        let gpus = self.gpus.collect(config);
        collection_times_us[4] = elapsed_us(started);
        collection_times_us[5] = elapsed_us(collection_started);
        Ok(Sample {
            cpu,
            memory,
            network,
            processes,
            process_count,
            gpus,
            collection_times_us,
        })
    }

    fn collect_cpu(&mut self, config: &Config) -> Result<(CpuSample, u64), String> {
        if cfg!(target_os = "macos") {
            #[cfg(target_os = "macos")]
            return macos::collect_cpu(self, config);
        }
        let stat = fs::read_to_string("/proc/stat")
            .map_err(|e| format!("could not read /proc/stat: {e}"))?;
        let current = parse_cpu_stat(&stat)?;
        let percentages: Vec<f64> = current
            .iter()
            .enumerate()
            .map(|(index, now)| {
                let Some(now) = now else {
                    return 0.0;
                };
                let old = self.previous_cpu.get(index).copied().unwrap_or_default();
                let total = now.total.saturating_sub(old.total);
                let busy = now.busy.saturating_sub(old.busy);
                if total == 0 {
                    0.0
                } else {
                    busy as f64 * 100.0 / total as f64
                }
            })
            .collect();
        let total_delta = current
            .first()
            .and_then(Option::as_ref)
            .map(|now| {
                now.total
                    .saturating_sub(self.previous_cpu.first().copied().unwrap_or_default().total)
            })
            .unwrap_or(0);
        let core_count = percentages.len().saturating_sub(1);
        let field_names = [
            "user",
            "nice",
            "system",
            "idle",
            "iowait",
            "irq",
            "softirq",
            "steal",
            "guest",
            "guest_nice",
        ];
        let fields = current
            .first()
            .and_then(Option::as_ref)
            .map(|now| {
                let old = self.previous_cpu.first().copied().unwrap_or_default();
                field_names
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| {
                        let value = if total_delta == 0 {
                            0.0
                        } else {
                            (now.fields[index].saturating_sub(old.fields[index]) as f64 * 100.0
                                / total_delta as f64)
                                .round()
                                .clamp(0.0, 100.0)
                        };
                        (name.to_string(), value)
                    })
                    .collect()
            })
            .unwrap_or_default();
        self.previous_cpu = current
            .into_iter()
            .enumerate()
            .map(|(index, current)| {
                current.unwrap_or_else(|| self.previous_cpu.get(index).copied().unwrap_or_default())
            })
            .collect();

        let load = fs::read_to_string("/proc/loadavg")
            .ok()
            .map(|text| {
                let mut parts = text
                    .split_whitespace()
                    .filter_map(|v| v.parse::<f64>().ok());
                [
                    parts.next().unwrap_or(0.0),
                    parts.next().unwrap_or(0.0),
                    parts.next().unwrap_or(0.0),
                ]
            })
            .unwrap_or([0.0; 3]);
        let uptime = fs::read_to_string("/proc/uptime")
            .ok()
            .and_then(|v| v.split_whitespace().next()?.parse::<f64>().ok())
            .unwrap_or(0.0) as u64;

        let watts = if config.bool_value("show_cpu_watts").unwrap_or(true) {
            self.read_cpu_watts()
        } else {
            None
        };
        let (temperature, temperature_max) = read_temperature_info(config).unwrap_or((0.0, 90.0));
        Ok((
            CpuSample {
                total: percentages.first().copied().unwrap_or(0.0),
                fields,
                cores: percentages.into_iter().skip(1).collect(),
                load,
                frequency: read_frequency(config.value("freq_mode").unwrap_or("first")),
                temperature: (temperature > 0.0).then_some(temperature),
                temperature_max,
                core_temperatures: read_core_temperatures(core_count, config),
                name: self.cpu_name.clone(),
                uptime,
                battery: config
                    .bool_value("show_battery")
                    .unwrap_or(true)
                    .then(|| read_battery(config))
                    .flatten(),
                watts,
                container_engine: self.container_engine.clone(),
                active_cpus: read_active_cpus(core_count),
                available_batteries: battery_names(Path::new("/sys/class/power_supply")),
            },
            total_delta,
        ))
    }

    fn read_cpu_watts(&mut self) -> Option<f64> {
        let energy = fs::read_to_string("/sys/class/powercap/intel-rapl:0/energy_uj")
            .ok()?
            .trim()
            .parse::<u64>()
            .ok()?;
        let now = Instant::now();
        let watts = self.rapl_previous.and_then(|(previous, timestamp)| {
            let micros = now.duration_since(timestamp).as_micros() as f64;
            (micros > 0.0 && energy >= previous).then(|| (energy - previous) as f64 / micros)
        });
        self.rapl_previous = Some((energy, now));
        Some(watts.unwrap_or(0.0))
    }

    fn collect_network(&mut self, config: &Config, elapsed: f64) -> Result<NetworkSample, String> {
        if cfg!(target_os = "macos") {
            #[cfg(target_os = "macos")]
            return macos::collect_network(self, config, elapsed);
        }
        let text = fs::read_to_string("/proc/net/dev")
            .map_err(|e| format!("could not read /proc/net/dev: {e}"))?;
        let mut counters = HashMap::new();
        for line in text.lines().skip(2) {
            let Some((name, values)) = line.split_once(':') else {
                continue;
            };
            let fields: Vec<u64> = values
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if fields.len() >= 16 {
                counters.insert(name.trim().to_string(), (fields[0], fields[8]));
            }
        }
        let mut interfaces: Vec<String> = counters.keys().cloned().collect();
        interfaces.sort();
        let connected = |name: &str| interface_running(name);
        let selected = config
            .net_iface
            .as_ref()
            .filter(|name| counters.contains_key(*name))
            .cloned()
            .or_else(|| {
                interfaces
                    .iter()
                    .filter(|name| connected(name))
                    .max_by_key(|name| {
                        let (receive, transmit) = counters.get(*name).copied().unwrap_or_default();
                        let old = self
                            .previous_network
                            .get(*name)
                            .copied()
                            .unwrap_or_default();
                        receive
                            .saturating_add(old.receive_rollover)
                            .saturating_add(transmit)
                            .saturating_add(old.transmit_rollover)
                    })
                    .cloned()
            })
            .or_else(|| interfaces.first().cloned())
            .unwrap_or_default();
        let mut speeds = HashMap::new();
        let mut totals = HashMap::new();
        for (interface, (receive, transmit)) in &counters {
            let had_previous = self.previous_network.contains_key(interface);
            let saved = self.previous_network.entry(interface.clone()).or_default();
            let (receive_speed, receive_total) = update_network_counter(
                *receive,
                &mut saved.receive_last,
                &mut saved.receive_rollover,
                elapsed,
                had_previous,
            );
            let (transmit_speed, transmit_total) = update_network_counter(
                *transmit,
                &mut saved.transmit_last,
                &mut saved.transmit_rollover,
                elapsed,
                had_previous,
            );
            speeds.insert(interface.clone(), (receive_speed, transmit_speed));
            totals.insert(interface.clone(), (receive_total, transmit_total));
        }
        self.previous_network
            .retain(|interface, _| counters.contains_key(interface));
        let (download_per_second, upload_per_second) =
            speeds.get(&selected).copied().unwrap_or_default();
        let (downloaded, uploaded) = totals.get(&selected).copied().unwrap_or_default();
        let is_connected = counters.contains_key(&selected) && connected(&selected);
        Ok(NetworkSample {
            interfaces,
            selected: selected.clone(),
            download_per_second,
            upload_per_second,
            downloaded,
            uploaded,
            ipv4: interface_ipv4(&selected).or_else(|| interface_hardware_address(&selected)),
            ipv6: interface_ipv6(&selected),
            connected: is_connected,
        })
    }

    fn collect_processes(
        &mut self,
        total_delta: u64,
        cores: usize,
        total_memory: u64,
        config: &Config,
        detailed_pid: Option<u32>,
    ) -> Result<Vec<ProcessSample>, String> {
        if cfg!(target_os = "macos") {
            #[cfg(target_os = "macos")]
            return macos::collect_processes(self, total_delta, cores, config, detailed_pid);
        }
        let entries =
            fs::read_dir("/proc").map_err(|e| format!("could not enumerate /proc: {e}"))?;
        let mut next_ticks = HashMap::new();
        let mut result = Vec::new();
        for entry in entries.flatten() {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|v| v.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(stat) = fs::read_to_string(entry.path().join("stat")) else {
                continue;
            };
            let Some(raw) = parse_process_stat(pid, &stat) else {
                continue;
            };
            let ticks = raw.user_ticks.saturating_add(raw.system_ticks);
            next_ticks.insert(pid, ticks);
            let delta =
                ticks.saturating_sub(self.previous_processes.get(&pid).copied().unwrap_or(ticks));
            let mut cpu = if total_delta == 0 {
                0.0
            } else {
                delta as f64 * 100.0 / total_delta as f64
            };
            if config.process_per_core {
                cpu *= cores as f64;
            }
            cpu = (cpu * 10.0).round() / 10.0;
            let uptime_ticks = read_uptime() * clock_ticks() as f64;
            let elapsed_ticks = (uptime_ticks - raw.start_ticks as f64).max(1.0);
            let cumulative_cpu = ticks as f64 * 100.0 / elapsed_ticks;
            let elapsed_seconds = (elapsed_ticks / clock_ticks() as f64) as u64;
            let status = fs::read_to_string(entry.path().join("status")).unwrap_or_default();
            let uid = status
                .lines()
                .find_map(|line| {
                    line.strip_prefix("Uid:")?
                        .split_whitespace()
                        .next()?
                        .parse::<u32>()
                        .ok()
                })
                .unwrap_or(0);
            let stat_memory = raw.rss_pages.saturating_mul(page_size());
            let memory = if detailed_pid == Some(pid)
                && config.bool_value("proc_info_smaps").unwrap_or(false)
            {
                read_smaps_rss(&entry.path().join("smaps")).unwrap_or(stat_memory)
            } else if total_memory > 0 && stat_memory >= total_memory {
                read_statm_rss(&entry.path().join("statm")).unwrap_or(stat_memory)
            } else {
                stat_memory
            };
            let command_bytes = fs::read(entry.path().join("cmdline")).unwrap_or_default();
            let kernel_thread = pid == 2 || raw.parent == 2;
            let command = (!command_bytes.is_empty())
                .then(|| {
                    let bytes = &command_bytes[..command_bytes.len().min(1_000)];
                    String::from_utf8_lossy(bytes)
                        .split('\0')
                        .filter(|v| !v.is_empty())
                        .collect::<Vec<_>>()
                        .join(" ")
                })
                .filter(|v| !v.is_empty())
                .unwrap_or_default();
            let (read_bytes, write_bytes) = read_process_io(pid);
            result.push(ProcessSample {
                pid,
                parent: raw.parent,
                name: raw.name,
                command,
                user: self
                    .users
                    .get(&uid)
                    .cloned()
                    .unwrap_or_else(|| uid.to_string()),
                state: raw.state,
                threads: raw.threads,
                memory,
                cpu,
                cumulative_cpu,
                nice: raw.nice,
                kernel_thread,
                elapsed_seconds,
                read_bytes,
                write_bytes,
            });
        }
        self.previous_processes = next_ticks;
        Ok(result)
    }
}

fn elapsed_us(started: Instant) -> u64 {
    started.elapsed().as_micros().min(u64::MAX as u128) as u64
}

fn read_smaps_rss(path: &Path) -> Option<u64> {
    let text = fs::read_to_string(path).ok()?;
    let kibibytes = text
        .lines()
        .filter_map(|line| line.strip_prefix("Rss:"))
        .filter_map(|value| value.split_whitespace().next())
        .filter_map(|value| value.parse::<u64>().ok())
        .sum::<u64>();
    (kibibytes > 0).then_some(kibibytes.saturating_mul(1024))
}

fn read_statm_rss(path: &Path) -> Option<u64> {
    fs::read_to_string(path)
        .ok()?
        .split_whitespace()
        .nth(1)?
        .parse::<u64>()
        .ok()
        .map(|pages| pages.saturating_mul(page_size()))
}

fn read_process_io(pid: u32) -> (u64, u64) {
    let text = fs::read_to_string(format!("/proc/{pid}/io")).unwrap_or_default();
    let mut read = 0;
    let mut write = 0;
    for line in text.lines() {
        let Some((name, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().parse().unwrap_or(0);
        match name {
            "read_bytes" => read = value,
            "write_bytes" => write = value,
            _ => {}
        }
    }
    (read, write)
}

fn parse_cpu_ticks(line: &str) -> Option<CpuTicks> {
    let mut fields = line.split_whitespace();
    fields.next()?;
    let values: Vec<u64> = fields.filter_map(|v| v.parse().ok()).take(10).collect();
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total: u64 = values.iter().take(8).sum();
    let mut field_values = [0; 10];
    for (target, value) in field_values.iter_mut().zip(values) {
        *target = value;
    }
    Some(CpuTicks {
        busy: total.saturating_sub(idle),
        total,
        fields: field_values,
    })
}

fn parse_cpu_stat(text: &str) -> Result<Vec<Option<CpuTicks>>, String> {
    let mut aggregate = None;
    let mut cores = Vec::new();
    for line in text.lines().take_while(|line| line.starts_with("cpu")) {
        let name = line.split_whitespace().next().unwrap_or_default();
        let ticks = parse_cpu_ticks(line);
        if name == "cpu" {
            aggregate = ticks;
            continue;
        }
        let Some(index) = name
            .strip_prefix("cpu")
            .and_then(|value| value.parse().ok())
        else {
            continue;
        };
        if cores.len() <= index {
            cores.resize(index + 1, None);
        }
        cores[index] = ticks;
    }
    let Some(aggregate) = aggregate else {
        return Err("Failed to parse /proc/stat".into());
    };
    let mut current = Vec::with_capacity(cores.len() + 1);
    current.push(Some(aggregate));
    current.extend(cores);
    Ok(current)
}

struct RawProcess {
    name: String,
    state: char,
    parent: u32,
    user_ticks: u64,
    system_ticks: u64,
    threads: u32,
    rss_pages: u64,
    nice: i32,
    start_ticks: u64,
}

fn parse_process_stat(_pid: u32, stat: &str) -> Option<RawProcess> {
    let open = stat.find('(')?;
    let close = stat.rfind(')')?;
    let name = stat[open + 1..close].to_string();
    let rest: Vec<&str> = stat[close + 2..].split_whitespace().collect();
    Some(RawProcess {
        name,
        state: rest.first()?.chars().next()?,
        parent: rest.get(1)?.parse().ok()?,
        user_ticks: rest.get(11)?.parse().ok()?,
        system_ticks: rest.get(12)?.parse().ok()?,
        threads: rest.get(17)?.parse().ok()?,
        rss_pages: rest.get(21)?.parse::<i64>().ok()?.max(0) as u64,
        nice: rest.get(16)?.parse().ok()?,
        start_ticks: rest.get(19)?.parse().ok()?,
    })
}

fn collect_memory(
    config: &Config,
    previous_disks: &mut HashMap<String, DiskCounters>,
    elapsed: f64,
) -> Result<MemorySample, String> {
    if cfg!(target_os = "macos") {
        #[cfg(target_os = "macos")]
        return macos::collect_memory(config, previous_disks, elapsed);
    }
    let text = fs::read_to_string("/proc/meminfo")
        .map_err(|e| format!("could not read /proc/meminfo: {e}"))?;
    let mut values = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Some(number) = value
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<u64>().ok())
        {
            values.insert(key, number.saturating_mul(1024));
        }
    }
    let total = values.get("MemTotal").copied().unwrap_or(0);
    let available = values.get("MemAvailable").copied().unwrap_or(0);
    let free = values.get("MemFree").copied().unwrap_or(0);
    let mut cached = values.get("Cached").copied().unwrap_or(0);
    let mut available = available;
    if config.bool_value("zfs_arc_cached").unwrap_or(true)
        && let Some((arc_size, arc_min)) =
            read_zfs_arcstats(Path::new("/proc/spl/kstat/zfs/arcstats"))
    {
        cached = cached.saturating_add(arc_size);
        available = available.saturating_add(arc_size.saturating_sub(arc_min));
    }
    let swap_total = values.get("SwapTotal").copied().unwrap_or(0);
    let swap_free = values.get("SwapFree").copied().unwrap_or(0);
    Ok(MemorySample {
        total,
        used: total.saturating_sub(if available <= total { available } else { free }),
        free,
        available,
        cached,
        swap_total,
        swap_used: swap_total.saturating_sub(swap_free),
        disks: collect_disks(config, previous_disks, elapsed),
    })
}

fn collect_disks(
    config: &Config,
    previous_disks: &mut HashMap<String, DiskCounters>,
    elapsed: f64,
) -> Vec<DiskSample> {
    let mounts = fs::read_to_string("/etc/mtab")
        .or_else(|_| fs::read_to_string("/proc/self/mounts"))
        .unwrap_or_default();
    let use_fstab = config.bool_value("use_fstab").unwrap_or(false);
    let only_physical = config.bool_value("only_physical").unwrap_or(true) && !use_fstab;
    let free_priv = config.bool_value("disk_free_priv").unwrap_or(false);
    let physical_filesystems = physical_filesystems();
    let fstab_mounts = use_fstab.then(fstab_mounts).unwrap_or_default();
    let (exclude_filter, disk_filters) =
        disk_filters(config.value("disks_filter").unwrap_or_default());
    let mut seen = HashSet::new();
    let mut disks = Vec::new();
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let device = decode_mount_field(parts.next().unwrap_or(""));
        let mount = decode_mount_field(parts.next().unwrap_or(""));
        let filesystem = parts.next().unwrap_or("");
        let hide_zfs_datasets = config.bool_value("zfs_hide_datasets").unwrap_or(false);
        let zfs_dataset = filesystem == "zfs" && device.contains('/');
        if !seen.insert(mount.clone())
            || (only_physical && !physical_filesystems.contains(filesystem))
            || (use_fstab && !fstab_mounts.contains(&mount))
            || (!disk_filters.is_empty() && (disk_filters.contains(&mount) == exclude_filter))
            || (hide_zfs_datasets && zfs_dataset)
        {
            continue;
        }
        if let Some((total, free, available)) = stat_vfs(Path::new(&mount)) {
            let shown_free = if free_priv { free } else { available };
            let device_name = fs::canonicalize(&device)
                .unwrap_or_else(|_| Path::new(&device).to_path_buf())
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("")
                .to_string();
            let (counters, block_size) = if filesystem == "zfs" {
                (
                    read_zfs_counters(Path::new("/proc/spl/kstat/zfs"), &device, hide_zfs_datasets),
                    1,
                )
            } else {
                (
                    (!device_name.is_empty())
                        .then(|| {
                            fs::read_to_string(format!("/sys/class/block/{device_name}/stat")).ok()
                        })
                        .flatten()
                        .and_then(|text| parse_disk_stat(&text)),
                    512,
                )
            };
            let counter_key = if filesystem == "zfs" {
                format!("zfs:{device}")
            } else {
                device_name.clone()
            };
            let old = counters.and_then(|_| previous_disks.get(&counter_key).copied());
            let (read_per_second, write_per_second, io_activity) = old
                .zip(counters)
                .map(|(old, now)| {
                    disk_counter_delta(old, now, block_size, filesystem == "zfs", elapsed)
                })
                .unwrap_or_default();
            if let Some(counters) = counters {
                previous_disks.insert(counter_key, counters);
            }
            disks.push(DiskSample {
                mount,
                total,
                used: total.saturating_sub(shown_free),
                free: shown_free,
                io_supported: counters.is_some(),
                read_per_second,
                write_per_second,
                io_activity,
            });
        }
    }
    if let Some(root) = disks.iter().position(|disk| disk.mount == "/") {
        let root = disks.remove(root);
        disks.insert(0, root);
    }
    disks
}

fn disk_counter_delta(
    old: DiskCounters,
    now: DiskCounters,
    block_size: u64,
    zfs: bool,
    elapsed: f64,
) -> (u64, u64, f64) {
    let read = now
        .sectors_read
        .saturating_sub(old.sectors_read)
        .saturating_mul(block_size);
    let write = now
        .sectors_written
        .saturating_sub(old.sectors_written)
        .saturating_mul(block_size);
    let activity_delta = now.io_milliseconds.saturating_sub(old.io_milliseconds) as f64;
    let activity = if zfs {
        activity_delta
    } else {
        (activity_delta / elapsed.max(0.001) / 10.0).clamp(0.0, 100.0)
    };
    (read, write, activity)
}

fn read_zfs_arcstats(path: &Path) -> Option<(u64, u64)> {
    let text = fs::read_to_string(path).ok()?;
    let mut size = None;
    let mut minimum = None;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(name) = fields.next() else {
            continue;
        };
        let _kind = fields.next();
        let value = fields.next().and_then(|value| value.parse::<u64>().ok());
        match name {
            "size" => size = value,
            "c_min" => minimum = value,
            _ => {}
        }
    }
    Some((size?, minimum.unwrap_or(0)))
}

fn read_zfs_counters(root: &Path, device: &str, pool_total: bool) -> Option<DiskCounters> {
    let pool = device.split('/').next()?;
    let entries = fs::read_dir(root.join(pool)).ok()?;
    let mut total = DiskCounters::default();
    let mut matched = 0_u64;
    for entry in entries.flatten() {
        if !entry.file_name().to_string_lossy().starts_with("objset") {
            continue;
        }
        let Ok(text) = fs::read_to_string(entry.path()) else {
            continue;
        };
        let Some((name, counters)) = parse_zfs_objset(&text) else {
            continue;
        };
        if pool_total || name == device {
            total.sectors_read = total.sectors_read.saturating_add(counters.sectors_read);
            total.sectors_written = total
                .sectors_written
                .saturating_add(counters.sectors_written);
            total.io_milliseconds = total
                .io_milliseconds
                .saturating_add(counters.io_milliseconds);
            matched += 1;
            if !pool_total {
                break;
            }
        }
    }
    (matched > 0).then_some(total)
}

fn parse_zfs_objset(text: &str) -> Option<(String, DiskCounters)> {
    let mut name = None;
    let mut reads = 0;
    let mut writes = 0;
    let mut bytes_read = 0;
    let mut bytes_written = 0;
    for line in text.lines() {
        let mut fields = line.split_whitespace();
        let Some(label) = fields.next() else {
            continue;
        };
        let _kind = fields.next();
        let Some(value) = fields.next() else {
            continue;
        };
        match label {
            "dataset_name" => name = Some(value.to_string()),
            "reads" => reads = value.parse().unwrap_or(0),
            "writes" => writes = value.parse().unwrap_or(0),
            "nread" => bytes_read = value.parse().unwrap_or(0),
            "nwritten" => bytes_written = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    Some((
        name?,
        DiskCounters {
            sectors_read: bytes_read,
            sectors_written: bytes_written,
            io_milliseconds: reads + writes,
        },
    ))
}

fn disk_filters(value: &str) -> (bool, HashSet<String>) {
    let mut entries = value.split_whitespace();
    let Some(first) = entries.next() else {
        return (false, HashSet::new());
    };
    let exclude = first.starts_with("exclude=");
    let first = first.strip_prefix("exclude=").unwrap_or(first);
    let mut filters = HashSet::new();
    if !first.is_empty() {
        filters.insert(first.to_string());
    }
    filters.extend(entries.map(str::to_string));
    (exclude, filters)
}

fn decode_mount_field(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'\\'
            && index + 3 < bytes.len()
            && bytes[index + 1..=index + 3]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
        {
            let byte = (bytes[index + 1] - b'0') * 64
                + (bytes[index + 2] - b'0') * 8
                + (bytes[index + 3] - b'0');
            decoded.push(byte);
            index += 4;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn physical_filesystems() -> HashSet<String> {
    let mut filesystems: HashSet<String> = fs::read_to_string("/proc/filesystems")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let fields: Vec<_> = line.split_whitespace().collect();
            (fields.len() == 1 && !matches!(fields[0], "squashfs" | "nullfs"))
                .then(|| fields[0].to_string())
        })
        .collect();
    filesystems.extend(["zfs", "wslfs", "drvfs"].map(str::to_string));
    filesystems
}

fn fstab_mounts() -> HashSet<String> {
    fs::read_to_string("/etc/fstab")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                return None;
            }
            let mount = decode_mount_field(line.split_whitespace().nth(1)?);
            (!matches!(mount.as_str(), "none" | "swap")).then_some(mount)
        })
        .collect()
}

fn parse_disk_stat(text: &str) -> Option<DiskCounters> {
    let fields: Vec<u64> = text
        .split_whitespace()
        .map(str::parse)
        .collect::<Result<_, _>>()
        .ok()?;
    Some(DiskCounters {
        sectors_read: *fields.get(2)?,
        sectors_written: *fields.get(6)?,
        io_milliseconds: *fields.get(9)?,
    })
}

fn read_cpu_name() -> String {
    let text = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let raw = text
        .lines()
        .find_map(|line| {
            let (key, value) = line.split_once(':')?;
            if matches!(key.trim(), "model name" | "Hardware" | "Processor") {
                Some(value.trim().to_string())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "Unknown CPU".into());
    clean_cpu_name(&raw)
}

fn clean_cpu_name(raw: &str) -> String {
    let parts = raw.split_whitespace().collect::<Vec<_>>();
    let cpu_position = parts.iter().position(|part| *part == "CPU");
    let mut name = if let Some(position) =
        cpu_position.filter(|_| raw.contains("Xeon") || parts.contains(&"Duo"))
    {
        parts
            .get(position + 1)
            .filter(|part| !part.ends_with(')'))
            .copied()
            .unwrap_or_default()
            .to_string()
    } else if let Some(position) = parts.iter().position(|part| *part == "Ryzen") {
        let mut selected = vec!["Ryzen"];
        let mut tokens = 0;
        for part in parts.iter().skip(position + 1) {
            if !matches!(*part, "AI" | "PRO" | "H" | "HX") {
                tokens += 1;
            }
            selected.push(part);
            if tokens >= 2 {
                break;
            }
        }
        selected.join(" ")
    } else if let Some(position) = cpu_position.filter(|_| raw.contains("Intel")) {
        parts
            .get(position + 1)
            .filter(|part| !part.ends_with(')') && **part != "@")
            .copied()
            .unwrap_or_default()
            .to_string()
    } else {
        String::new()
    };

    if name.is_empty() && !parts.is_empty() {
        name = parts
            .iter()
            .take_while(|part| **part != "@")
            .copied()
            .collect::<Vec<_>>()
            .join(" ");
        for remove in [
            "Processor",
            "CPU",
            "(R)",
            "(TM)",
            "Intel",
            "AMD",
            "Apple",
            "Core",
        ] {
            name = name.replace(remove, "");
            while name.contains("  ") {
                name = name.replace("  ", " ");
            }
        }
        name = name.trim().to_string();
    }
    name
}

fn read_frequency(mode: &str) -> String {
    let mut policies = fs::read_dir("/sys/devices/system/cpu/cpufreq")
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("policy"))
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    policies.sort();
    let mut frequencies = policies
        .into_iter()
        .filter_map(|entry| {
            fs::read_to_string(entry.join("scaling_cur_freq"))
                .ok()?
                .trim()
                .parse::<f64>()
                .ok()
                .map(|khz| khz / 1000.0)
        })
        .filter(|frequency| *frequency > 0.0)
        .collect::<Vec<_>>();
    if mode == "first" {
        frequencies.truncate(1);
    }
    if frequencies.is_empty() {
        frequencies = fs::read_to_string("/proc/cpuinfo")
            .unwrap_or_default()
            .lines()
            .filter_map(|line| {
                let (key, value) = line.split_once(':')?;
                key.trim()
                    .eq_ignore_ascii_case("cpu MHz")
                    .then(|| value.trim().parse::<f64>().ok())
                    .flatten()
            })
            .take(if mode == "first" { 1 } else { usize::MAX })
            .collect();
    }
    calculate_frequency(mode, &frequencies)
}

fn calculate_frequency(mode: &str, frequencies: &[f64]) -> String {
    let Some(first) = frequencies.first().copied() else {
        return String::new();
    };
    if mode == "range" {
        let lowest = frequencies.iter().copied().fold(first, f64::min);
        let highest = frequencies.iter().copied().fold(first, f64::max);
        return format!(
            "{} - {}",
            normalize_frequency(lowest),
            normalize_frequency(highest)
        );
    }
    let frequency = match mode {
        "average" => frequencies.iter().sum::<f64>() / frequencies.len() as f64,
        "highest" => frequencies.iter().copied().fold(first, f64::max),
        "lowest" => frequencies.iter().copied().fold(first, f64::min),
        _ => first,
    };
    normalize_frequency(frequency)
}

fn normalize_frequency(mhz: f64) -> String {
    if mhz > 999_999.0 {
        short_decimal(mhz / 1_000_000.0, "THz")
    } else if mhz > 999.0 {
        short_decimal(mhz / 1_000.0, "GHz")
    } else {
        format!("{mhz:.0} MHz")
    }
}

fn short_decimal(value: f64, suffix: &str) -> String {
    let mut number = format!("{value:.1}");
    number.truncate(number.len().min(3));
    if number.ends_with('.') {
        number.pop();
    }
    format!("{number} {suffix}")
}

#[derive(Debug)]
struct TemperatureSensor {
    name: String,
    label: String,
    path: PathBuf,
    critical: f64,
}

fn hardware_temperature_sensors() -> Vec<TemperatureSensor> {
    let mut sensors = Vec::new();
    for path in hardware_sensor_paths() {
        let provider = fs::read_to_string(path.join("name"))
            .unwrap_or_else(|_| {
                path.file_name()
                    .unwrap_or_default()
                    .to_string_lossy()
                    .into_owned()
            })
            .trim()
            .to_string();
        if provider == "nvme" || path.to_string_lossy().contains("nvme") {
            continue;
        }
        for index in 1..128 {
            let input = path.join(format!("temp{index}_input"));
            if !input.exists() {
                continue;
            }
            let label = fs::read_to_string(path.join(format!("temp{index}_label")))
                .unwrap_or_else(|_| format!("temp{index}"))
                .trim()
                .to_string();
            sensors.push(TemperatureSensor {
                name: format!("{provider}/{label}"),
                label,
                path: input,
                critical: read_number(path.join(format!("temp{index}_crit")))
                    .map(|value| value / 1000.0)
                    .filter(|value| *value > 0.0)
                    .unwrap_or(95.0),
            });
        }
    }
    sensors
}

fn hardware_sensor_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    if let Ok(entries) = fs::read_dir("/sys/class/hwmon") {
        for entry in entries.flatten() {
            let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            if seen.contains(&path) || seen.contains(&path.join("device")) {
                continue;
            }
            for candidate in [path.join("device"), path] {
                let has_temperature = fs::read_dir(&candidate).is_ok_and(|entries| {
                    entries.flatten().any(|entry| {
                        let name = entry.file_name();
                        let name = name.to_string_lossy();
                        name.starts_with("temp") && name.ends_with("_input")
                    })
                });
                if has_temperature && seen.insert(candidate.clone()) {
                    paths.push(candidate);
                }
            }
        }
    }
    if !paths
        .iter()
        .any(|path| path.to_string_lossy().contains("coretemp"))
        && let Ok(entries) = fs::read_dir("/sys/devices/platform/coretemp.0/hwmon")
    {
        for entry in entries.flatten() {
            let path = fs::canonicalize(entry.path()).unwrap_or_else(|_| entry.path());
            if seen.insert(path.clone()) {
                paths.push(path);
            }
        }
    }
    paths
}

fn thermal_temperature_sensors() -> Vec<TemperatureSensor> {
    let mut sensors = Vec::new();
    let Ok(entries) = fs::read_dir("/sys/class/thermal") else {
        return sensors;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(index) = name.strip_prefix("thermal_zone") else {
            continue;
        };
        let path = entry.path();
        if !path.join("temp").exists() {
            continue;
        }
        let label = fs::read_to_string(path.join("type"))
            .unwrap_or_else(|_| format!("temp{index}"))
            .trim()
            .to_string();
        sensors.push(TemperatureSensor {
            name: format!("thermal{index}/{label}"),
            label,
            path: path.join("temp"),
            critical: thermal_critical(&path),
        });
    }
    sensors
}

fn is_primary_cpu_sensor(sensor: &TemperatureSensor) -> bool {
    sensor.label.starts_with("Package id")
        || sensor.label.starts_with("Tdie")
        || sensor.label.starts_with("SoC Temperature")
}

fn available_temperature_sensors() -> Vec<TemperatureSensor> {
    let mut sensors = hardware_temperature_sensors();
    if !sensors.iter().any(is_primary_cpu_sensor) {
        sensors.extend(thermal_temperature_sensors());
    }
    sensors
}

pub fn temperature_sensor_names() -> Vec<String> {
    let mut names = vec!["Auto".to_string()];
    names.extend(
        available_temperature_sensors()
            .into_iter()
            .map(|sensor| sensor.name),
    );
    names.sort_by(|a, b| {
        (a != "Auto")
            .cmp(&(b != "Auto"))
            .then_with(|| a.len().cmp(&b.len()))
            .then_with(|| a.cmp(b))
    });
    names.dedup();
    names
}

fn read_temperature(config: &Config) -> Option<f64> {
    read_temperature_info(config).map(|(temperature, _)| temperature)
}

fn read_temperature_info(config: &Config) -> Option<(f64, f64)> {
    let sensors = available_temperature_sensors();
    let configured = config.value("cpu_sensor").filter(|value| !value.is_empty());
    let sensor = configured
        .and_then(|configured| sensors.iter().find(|sensor| sensor.name == configured))
        .or_else(|| sensors.iter().find(|sensor| is_primary_cpu_sensor(sensor)))
        .or_else(|| {
            sensors.iter().find(|sensor| {
                let name = sensor.name.to_ascii_lowercase();
                name.contains("cpu") || name.contains("k10temp")
            })
        });
    if let Some(sensor) = sensor
        && let Some(value) = read_number(&sensor.path)
    {
        return Some((value / 1000.0, sensor.critical));
    }
    let entries = fs::read_dir("/sys/class/thermal").ok()?;
    let mut fallback = None;
    for entry in entries.flatten() {
        if !entry
            .file_name()
            .to_string_lossy()
            .starts_with("thermal_zone")
        {
            continue;
        }
        let temp = fs::read_to_string(entry.path().join("temp"))
            .ok()?
            .trim()
            .parse::<f64>()
            .ok()?
            / 1000.0;
        let kind = fs::read_to_string(entry.path().join("type"))
            .unwrap_or_default()
            .to_ascii_lowercase();
        if kind.contains("package") || kind.contains("cpu") || kind.contains("x86_pkg") {
            return Some((temp, thermal_critical(&entry.path())));
        }
        fallback.get_or_insert((temp, thermal_critical(&entry.path())));
    }
    fallback
}

fn thermal_critical(path: &Path) -> f64 {
    for index in 0..128 {
        let kind =
            fs::read_to_string(path.join(format!("trip_point_{index}_type"))).unwrap_or_default();
        if matches!(kind.trim(), "critical" | "high")
            && let Some(value) = read_number(path.join(format!("trip_point_{index}_temp")))
        {
            return value / 1000.0;
        }
    }
    95.0
}

fn read_core_temperatures(core_count: usize, config: &Config) -> Vec<Option<f64>> {
    let mut core_sensors = hardware_temperature_sensors()
        .into_iter()
        .filter(|sensor| sensor.label.starts_with("Core") || sensor.label.starts_with("Tccd"))
        .collect::<Vec<_>>();
    core_sensors.sort_by(|a, b| {
        a.name
            .len()
            .cmp(&b.name.len())
            .then_with(|| a.name.cmp(&b.name))
    });
    let temperatures = core_sensors
        .iter()
        .map(|sensor| read_number(&sensor.path).map(|value| value / 1000.0))
        .collect::<Vec<_>>();
    if temperatures.is_empty() {
        return vec![read_temperature(config); core_count];
    }
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let mut mapping = cpu_core_sensor_mapping(&cpuinfo, core_count, temperatures.len());
    if let Some(custom) = config.value("cpu_core_map") {
        for pair in custom.split_whitespace() {
            let Some((core, sensor)) = pair.split_once(':') else {
                continue;
            };
            let (Ok(core), Ok(sensor)) = (core.parse::<usize>(), sensor.parse::<usize>()) else {
                continue;
            };
            if core < mapping.len() && sensor < temperatures.len() {
                mapping[core] = sensor;
            }
        }
    }
    mapping
        .into_iter()
        .map(|sensor| temperatures.get(sensor).copied().flatten())
        .collect()
}

fn cpu_core_sensor_mapping(text: &str, core_count: usize, sensor_count: usize) -> Vec<usize> {
    if sensor_count == 0 {
        return vec![0; core_count];
    }
    let mut cpu = None;
    let mut cpu_to_core = HashMap::new();
    let mut maximum_core = 0;
    for line in text.lines() {
        let Some((label, value)) = line.split_once(':') else {
            continue;
        };
        match label.trim() {
            "processor" => cpu = value.trim().parse::<usize>().ok(),
            "core id" => {
                if let (Some(cpu), Ok(core)) = (cpu, value.trim().parse::<usize>()) {
                    cpu_to_core.insert(cpu, core);
                    maximum_core = maximum_core.max(core);
                }
            }
            _ => {}
        }
    }
    let mut mapping = HashMap::new();
    for (cpu, core) in cpu_to_core {
        mapping.insert(cpu, core * sensor_count / (maximum_core + 1));
    }
    if mapping.len() < core_count {
        if core_count.is_multiple_of(2) && mapping.len() == core_count / 2 {
            for cpu in 0..core_count / 2 {
                let sensor = mapping.get(&cpu).copied().unwrap_or(cpu % sensor_count);
                mapping.insert(core_count / 2 + cpu, sensor);
            }
        } else {
            mapping.clear();
            for cpu in 0..core_count {
                mapping.insert(cpu, cpu * sensor_count / core_count.max(1));
            }
        }
    }
    (0..core_count)
        .map(|cpu| mapping.get(&cpu).copied().unwrap_or(cpu % sensor_count))
        .collect()
}

fn update_network_counter(
    current: u64,
    last: &mut u64,
    rollover: &mut u64,
    elapsed: f64,
    had_previous: bool,
) -> (u64, u64) {
    if !had_previous {
        *last = current;
        return (0, current.saturating_add(*rollover));
    }
    if current < *last {
        *rollover = rollover.saturating_add(*last);
        *last = 0;
    }
    if rollover.checked_add(current).is_none() {
        *rollover = 0;
        *last = 0;
    }
    let speed = ((current.saturating_sub(*last) as f64) / elapsed.max(0.001)).round() as u64;
    *last = current;
    (speed, current.saturating_add(*rollover))
}

fn interface_running(iface: &str) -> bool {
    fs::read_to_string(format!("/sys/class/net/{iface}/operstate"))
        .map(|state| matches!(state.trim(), "up" | "unknown"))
        .unwrap_or(false)
}

fn interface_ipv4(iface: &str) -> Option<String> {
    let mut head: *mut IfAddrs = ptr::null_mut();
    if unsafe { getifaddrs(&mut head) } != 0 {
        return None;
    }
    let mut current = head;
    let mut result = None;
    while !current.is_null() {
        let address = unsafe { &*current };
        if !address.name.is_null() && !address.address.is_null() {
            let name = unsafe { std::ffi::CStr::from_ptr(address.name) }.to_string_lossy();
            let socket = unsafe { &*address.address };
            if name == iface && socket.family == AF_INET {
                // getifaddrs does not promise alignment for the concrete
                // sockaddr type behind this pointer.
                let ipv4 = unsafe { ptr::read_unaligned(address.address.cast::<SockAddrIn>()) };
                let bytes = ipv4.address.to_ne_bytes();
                result = Some(format!(
                    "{}.{}.{}.{}",
                    bytes[0], bytes[1], bytes[2], bytes[3]
                ));
                break;
            }
        }
        current = address.next;
    }
    unsafe { freeifaddrs(head) };
    result
}

fn interface_hardware_address(iface: &str) -> Option<String> {
    let address = fs::read_to_string(format!("/sys/class/net/{iface}/address"))
        .ok()?
        .trim()
        .to_string();
    (!address.is_empty()).then_some(address)
}

fn interface_ipv6(iface: &str) -> Option<String> {
    let text = fs::read_to_string("/proc/net/if_inet6").ok()?;
    text.lines().find_map(|line| {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 6 || fields[5] != iface || fields[0].len() != 32 {
            return None;
        }
        let mut segments = [0u16; 8];
        for (index, segment) in segments.iter_mut().enumerate() {
            *segment = u16::from_str_radix(&fields[0][index * 4..index * 4 + 4], 16).ok()?;
        }
        Some(std::net::Ipv6Addr::from(segments).to_string())
    })
}

fn read_battery(config: &Config) -> Option<BatterySample> {
    read_battery_at(
        Path::new("/sys/class/power_supply"),
        config.value("selected_battery").unwrap_or("Auto"),
    )
}

fn read_battery_at(root: &Path, selected: &str) -> Option<BatterySample> {
    let mut batteries = fs::read_dir(root)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let kind = fs::read_to_string(path.join("type")).ok()?;
            let kind = kind.trim().to_string();
            if !matches!(kind.as_str(), "Battery" | "UPS") {
                return None;
            }
            if path.join("present").exists()
                && fs::read_to_string(path.join("present")).ok()?.trim() != "1"
            {
                return None;
            }
            Some((entry.file_name().to_string_lossy().into_owned(), kind, path))
        })
        .collect::<Vec<_>>();
    batteries.sort_by(|a, b| a.0.cmp(&b.0));
    let (_, _, path) = batteries
        .iter()
        .find(|(name, _, _)| selected != "Auto" && name == selected)
        .or_else(|| batteries.iter().find(|(_, kind, _)| kind == "Battery"))
        .or_else(|| batteries.first())?;

    let energy_now = read_number(path.join("energy_now"));
    let energy_full = read_number(path.join("energy_full"));
    let charge_now = read_number(path.join("charge_now"));
    let charge_full = read_number(path.join("charge_full"));
    let power_now = read_number(path.join("power_now")).filter(|value| *value >= 0.0);
    let current_now = read_number(path.join("current_now")).filter(|value| *value >= 0.0);
    let voltage_now = read_number(path.join("voltage_now")).filter(|value| *value >= 0.0);
    let percent = read_number(path.join("capacity"))
        .or_else(|| Some(energy_now? * 100.0 / energy_full?))
        .or_else(|| Some(charge_now? * 100.0 / charge_full?))?
        .round()
        .clamp(0.0, 100.0) as u8;
    let mut status = fs::read_to_string(path.join("status"))
        .unwrap_or_else(|_| "Unknown".into())
        .trim()
        .to_ascii_lowercase();
    if status == "unknown" {
        let online = [path.join("AC0/online"), path.join("AC/online")]
            .into_iter()
            .find_map(read_number);
        if online == Some(1.0) {
            status = if percent < 100 { "charging" } else { "full" }.into();
        } else if online == Some(0.0) {
            status = "discharging".into();
        }
    }
    let positive_power = power_now.filter(|power| *power > 0.0);
    let positive_current = current_now.filter(|current| *current > 0.0);
    let seconds = match status.as_str() {
        "charging" => energy_full
            .zip(energy_now)
            .zip(positive_power)
            .map(|((full, now), power)| ((full - now).max(0.0) / power * 3600.0) as u64)
            .or_else(|| {
                charge_full
                    .zip(charge_now)
                    .zip(positive_current)
                    .map(|((full, now), current)| ((full - now).max(0.0) / current * 3600.0) as u64)
            }),
        "full" => None,
        _ => energy_now
            .zip(positive_power)
            .map(|(energy, power)| (energy / power * 3600.0) as u64)
            .or_else(|| {
                charge_now
                    .zip(positive_current)
                    .map(|(charge, current)| (charge / current * 3600.0) as u64)
            })
            .or_else(|| read_number(path.join("time_to_empty")).map(|minutes| minutes as u64 * 60)),
    }
    .filter(|seconds| *seconds > 0);
    let watts = power_now
        .map(|microwatts| microwatts / 1_000_000.0)
        .or_else(|| Some(current_now? * voltage_now? / 1_000_000_000_000.0));
    Some(BatterySample {
        percent,
        status,
        watts,
        seconds,
    })
}

fn battery_names(root: &Path) -> Vec<String> {
    let mut names = vec!["Auto".to_string()];
    names.extend(
        fs::read_dir(root)
            .into_iter()
            .flatten()
            .flatten()
            .filter_map(|entry| {
                let path = entry.path();
                let kind = fs::read_to_string(path.join("type")).ok()?;
                if !matches!(kind.trim(), "Battery" | "UPS") {
                    return None;
                }
                if path.join("present").exists()
                    && fs::read_to_string(path.join("present")).ok()?.trim() != "1"
                {
                    return None;
                }
                Some(entry.file_name().to_string_lossy().into_owned())
            }),
    );
    names[1..].sort();
    names
}

fn detect_container() -> Option<String> {
    if Path::new("/run/.containerenv").exists() {
        return Some("podman".into());
    }
    if Path::new("/.dockerenv").exists() {
        return Some("docker".into());
    }
    fs::read_to_string("/run/systemd/container")
        .ok()
        .and_then(|value| value.split_whitespace().next().map(str::to_string))
}

fn read_active_cpus(core_count: usize) -> Option<HashSet<usize>> {
    let text = fs::read_to_string("/sys/fs/cgroup/cpuset.cpus.effective").ok()?;
    if text.trim().is_empty() {
        return Some((0..core_count).collect());
    }
    parse_cpu_list(&text)
}

fn parse_cpu_list(text: &str) -> Option<HashSet<usize>> {
    let mut cpus = HashSet::new();
    for range in text.trim().split(',').filter(|part| !part.is_empty()) {
        if let Some((start, end)) = range.split_once('-') {
            let start = start.parse::<usize>().ok()?;
            let end = end.parse::<usize>().ok()?;
            if start > end {
                return None;
            }
            cpus.extend(start..=end);
        } else {
            cpus.insert(range.parse().ok()?);
        }
    }
    Some(cpus)
}

fn read_number(path: impl AsRef<Path>) -> Option<f64> {
    fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn read_users() -> HashMap<u32, String> {
    fs::read_to_string("/etc/passwd")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(':').collect();
            Some((fields.get(2)?.parse().ok()?, fields.first()?.to_string()))
        })
        .collect()
}

fn page_size() -> u64 {
    system_value(SC_PAGESIZE).max(1) as u64
}

fn clock_ticks() -> u64 {
    system_value(SC_CLK_TCK).max(1) as u64
}

fn read_uptime() -> f64 {
    fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|value| value.split_whitespace().next()?.parse().ok())
        .unwrap_or(0.0)
}

fn system_value(name: c_int) -> i64 {
    unsafe { sysconf(name) }
}

#[repr(C)]
#[derive(Default)]
struct StatVfs {
    block_size: c_ulong,
    fragment_size: c_ulong,
    blocks: c_ulong,
    blocks_free: c_ulong,
    blocks_available: c_ulong,
    files: c_ulong,
    files_free: c_ulong,
    files_available: c_ulong,
    filesystem_id: c_ulong,
    mount_flags: c_ulong,
    name_max: c_ulong,
    spare: [c_int; 6],
}

// getifaddrs uses platform-specific sockaddr layouts; the macOS collector has
// its own declaration and never calls this Linux-layout version on Darwin.
#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    fn statvfs(path: *const c_char, buf: *mut StatVfs) -> c_int;
    fn getifaddrs(addresses: *mut *mut IfAddrs) -> c_int;
    fn freeifaddrs(addresses: *mut IfAddrs);
    fn sysconf(name: c_int) -> i64;
}

const AF_INET: u16 = 2;
const SC_CLK_TCK: c_int = 2;
const SC_PAGESIZE: c_int = 30;

#[repr(C)]
struct IfAddrs {
    next: *mut IfAddrs,
    name: *mut c_char,
    _flags: c_uint,
    address: *mut SockAddr,
    _netmask: *mut SockAddr,
    _broad_or_dst: *mut SockAddr,
    _data: *mut std::ffi::c_void,
}

#[repr(C)]
struct SockAddr {
    family: u16,
    data: [u8; 14],
}

#[repr(C)]
struct SockAddrIn {
    _family: u16,
    _port: u16,
    address: u32,
    _zero: [u8; 8],
}

#[allow(clippy::useless_conversion)] // c_ulong is 32-bit on some supported Linux targets.
fn stat_vfs(path: &Path) -> Option<(u64, u64, u64)> {
    let path = CString::new(path.as_os_str().as_encoded_bytes()).ok()?;
    let mut stats = StatVfs::default();
    if unsafe { statvfs(path.as_ptr(), &mut stats) } != 0 {
        return None;
    }
    let fragment = u64::from(stats.fragment_size);
    Some((
        u64::from(stats.blocks).saturating_mul(fragment),
        u64::from(stats.blocks_free).saturating_mul(fragment),
        u64::from(stats.blocks_available).saturating_mul(fragment),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cpu_stat_preserves_missing_logical_cpu_slots() {
        let parsed = parse_cpu_stat(
            "cpu  100 2 30 400 5 6 7 8 9 10\ncpu0 40 1 10 200 2 3 4 5 6 7\ncpu2 60 1 20 200 3 3 3 3 3 3\nintr 1\n",
        )
        .unwrap();
        assert_eq!(parsed.len(), 4);
        assert!(parsed[0].is_some());
        assert!(parsed[1].is_some());
        assert!(parsed[2].is_none());
        assert!(parsed[3].is_some());
        assert_eq!(parsed[0].unwrap().total, 558);
        assert!(parse_cpu_stat("intr 1\n").is_err());
    }

    #[test]
    fn parses_process_names_with_spaces() {
        let stat = "42 (a process name) S 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 4 18 19 20 100";
        let parsed = parse_process_stat(42, stat).unwrap();
        assert_eq!(parsed.name, "a process name");
        assert_eq!(parsed.parent, 1);
    }

    #[test]
    fn parses_linux_block_io_counters() {
        let parsed = parse_disk_stat("12 3 400 5 6 7 800 9 0 1100 12").unwrap();
        assert_eq!(
            parsed,
            DiskCounters {
                sectors_read: 400,
                sectors_written: 800,
                io_milliseconds: 1100,
            }
        );
        assert!(parse_disk_stat("1 2 3").is_none());
    }

    #[test]
    fn disk_deltas_match_block_and_zfs_reference_accounting() {
        let old = DiskCounters {
            sectors_read: 100,
            sectors_written: 200,
            io_milliseconds: 1_000,
        };
        let now = DiskCounters {
            sectors_read: 110,
            sectors_written: 220,
            io_milliseconds: 1_500,
        };
        assert_eq!(
            disk_counter_delta(old, now, 512, false, 2.0),
            (5_120, 10_240, 25.0)
        );
        assert_eq!(disk_counter_delta(old, now, 1, true, 2.0), (10, 20, 500.0));
    }

    #[test]
    fn parses_include_and_exclude_disk_filters() {
        assert_eq!(
            disk_filters("/ /boot"),
            (false, HashSet::from(["/".into(), "/boot".into()]))
        );
        assert_eq!(
            disk_filters("exclude=/run /tmp"),
            (true, HashSet::from(["/run".into(), "/tmp".into()]))
        );
        assert_eq!(
            decode_mount_field("/media/a\\040b\\011c\\134d"),
            "/media/a b\tc\\d"
        );
    }

    #[test]
    fn network_counters_keep_totals_across_device_resets() {
        let mut last = 4_000;
        let mut rollover = 0;
        assert_eq!(
            update_network_counter(500, &mut last, &mut rollover, 0.5, true),
            (1_000, 4_500)
        );
        assert_eq!((last, rollover), (500, 4_000));
        assert_eq!(
            update_network_counter(1_000, &mut last, &mut rollover, 0.5, true),
            (1_000, 5_000)
        );
    }

    #[test]
    fn first_network_counter_sample_has_no_fake_speed() {
        let mut last = 0;
        let mut rollover = 0;
        assert_eq!(
            update_network_counter(12_345, &mut last, &mut rollover, 1.0, false),
            (0, 12_345)
        );
    }

    #[test]
    fn cpu_frequency_modes_match_btop_labels() {
        let frequencies = [2_000.0, 3_500.0, 4_000.0];
        assert_eq!(calculate_frequency("first", &frequencies), "2.0 GHz");
        assert_eq!(calculate_frequency("lowest", &frequencies), "2.0 GHz");
        assert_eq!(calculate_frequency("highest", &frequencies), "4.0 GHz");
        assert_eq!(calculate_frequency("average", &frequencies), "3.2 GHz");
        assert_eq!(
            calculate_frequency("range", &frequencies),
            "2.0 GHz - 4.0 GHz"
        );
    }

    #[test]
    fn cpu_core_sensor_mapping_uses_physical_core_ids_and_smt_fallback() {
        let cpuinfo = "processor : 0\ncore id : 0\n\
                       processor : 1\ncore id : 2\n\
                       processor : 2\ncore id : 4\n\
                       processor : 3\ncore id : 6\n";
        assert_eq!(
            cpu_core_sensor_mapping(cpuinfo, 8, 2),
            vec![0, 0, 1, 1, 0, 0, 1, 1]
        );
        assert_eq!(cpu_core_sensor_mapping("", 4, 2), vec![0, 0, 1, 1]);
    }

    #[test]
    fn selected_battery_and_ups_fallback_match_reference_selection() {
        let root = std::env::temp_dir().join(format!(
            "btoprs-battery-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for (name, kind, capacity) in [("BAT0", "Battery", "40"), ("UPS0", "UPS", "90")] {
            let path = root.join(name);
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("type"), kind).unwrap();
            fs::write(path.join("present"), "1").unwrap();
            fs::write(path.join("capacity"), capacity).unwrap();
            fs::write(path.join("status"), "Discharging").unwrap();
            fs::write(path.join("energy_now"), "20000000").unwrap();
            fs::write(path.join("energy_full"), "50000000").unwrap();
            fs::write(path.join("power_now"), "10000000").unwrap();
        }

        let automatic = read_battery_at(&root, "Auto").unwrap();
        assert_eq!(automatic.percent, 40);
        assert_eq!(automatic.seconds, Some(7_200));
        assert_eq!(automatic.watts, Some(10.0));
        assert_eq!(read_battery_at(&root, "UPS0").unwrap().percent, 90);
        assert_eq!(read_battery_at(&root, "missing").unwrap().percent, 40);
        assert_eq!(battery_names(&root), ["Auto", "BAT0", "UPS0"]);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn sums_rss_entries_from_linux_smaps() {
        let path = std::env::temp_dir().join(format!(
            "btoprs-smaps-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::write(
            &path,
            "1000-2000 r--p 0 0:0 0\nRss:                 12 kB\nPss:                  9 kB\n2000-3000 rw-p 0 0:0 0\nRss:                  7 kB\n",
        )
        .unwrap();

        assert_eq!(read_smaps_rss(&path), Some(19 * 1024));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn parses_effective_cpuset_ranges() {
        assert_eq!(
            parse_cpu_list("0-2,5,8-9\n"),
            Some(HashSet::from([0, 1, 2, 5, 8, 9]))
        );
        assert!(parse_cpu_list("4-2").is_none());
    }

    #[test]
    fn parses_zfs_arc_and_objset_accounting() {
        let root = std::env::temp_dir().join(format!(
            "btoprs-zfs-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let pool = root.join("tank");
        fs::create_dir_all(&pool).unwrap();
        let arc = root.join("arcstats");
        fs::write(&arc, "13 1 0x01 2 88 0\nc_min 4 1048576\nsize 4 4194304\n").unwrap();
        fs::write(
            pool.join("objset-1"),
            "13 1 0x01 2 88 0\nname type data\ndataset_name 7 tank/home\nreads 4 10\nnread 4 4096\nwrites 4 4\nnwritten 4 8192\n",
        )
        .unwrap();
        fs::write(
            pool.join("objset-2"),
            "dataset_name 7 tank/var\nreads 4 3\nnread 4 1024\nwrites 4 2\nnwritten 4 2048\n",
        )
        .unwrap();

        assert_eq!(read_zfs_arcstats(&arc), Some((4_194_304, 1_048_576)));
        assert_eq!(
            read_zfs_counters(&root, "tank/home", false),
            Some(DiskCounters {
                sectors_read: 4096,
                sectors_written: 8192,
                io_milliseconds: 14,
            })
        );
        assert_eq!(
            read_zfs_counters(&root, "tank", true),
            Some(DiskCounters {
                sectors_read: 5120,
                sectors_written: 10240,
                io_milliseconds: 19,
            })
        );

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn cpu_name_trimming_matches_upstream_fixtures() {
        for (input, expected) in [
            (
                "AMD Ryzen AI 7 PRO 360 w/ Radeon 880M",
                "Ryzen AI 7 PRO 360",
            ),
            (
                "AMD Ryzen 7 PRO 4750G with Radeon Graphics",
                "Ryzen 7 PRO 4750G",
            ),
            (
                "AMD Ryzen Threadripper PRO 3975WX 32-Cores",
                "Ryzen Threadripper PRO 3975WX",
            ),
            ("AMD Ryzen 7 5700X 8-Core Processor", "Ryzen 7 5700X"),
            ("AMD EPYC 7543 32-Core Processor", "EPYC 7543 32-"),
            ("Intel(R) Pentium(R) III CPU family 1400MHz", "family"),
            ("Intel(R) Pentium(R) CPU P6200 @ 2.13GHz", "P6200"),
            ("Intel(R) Core(TM) i7 CPU Q 840 @ 1.87GHz", "Q"),
            ("Intel(R) Core(TM) i5-4570 CPU @ 3.20GHz", "i5-4570"),
            ("12th Gen Intel(R) Core(TM) i5-12600", "12th Gen i5-12600"),
            ("Intel(R) Xeon(R) CPU E5-2690 v4 @ 2.60GHz", "E5-2690"),
            ("Intel(R) Xeon(R) Silver 4410Y", "Xeon Silver 4410Y"),
            ("Intel(R) Xeon(R) Gold 6138 CPU @ 2.00GHz", "@"),
            ("INTEL(R) XEON(R) GOLD 6548Y+", "INTEL XEON GOLD 6548Y+"),
        ] {
            assert_eq!(clean_cpu_name(input), expected, "{input}");
        }
    }
}
