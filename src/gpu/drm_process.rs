// Linux DRM client usage stats: docs.kernel.org/gpu/drm-usage-stats.html
use super::GpuSample;
use crate::{collect::ProcessSample, process_gpu::Snapshot};
use std::collections::HashMap;
use std::fs;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Driver {
    Amd,
    I915,
    Xe,
}

impl Driver {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "amdgpu" => Some(Self::Amd),
            "i915" => Some(Self::I915),
            "xe" => Some(Self::Xe),
            _ => None,
        }
    }

    fn engine(self, engine: &str) -> bool {
        let class = engine.trim_end_matches(|c: char| c.is_ascii_digit());
        match self {
            Self::Amd => matches!(class, "gfx" | "compute"),
            Self::I915 => matches!(class, "render" | "compute"),
            Self::Xe => matches!(class, "rcs" | "ccs"),
        }
    }
}

#[derive(Default)]
struct Counter {
    busy: Option<u64>,
    total: Option<u64>,
    capacity: Option<u64>,
}

struct Client {
    id: u64,
    bytes: Option<u64>,
    dedicated: Option<u64>,
    engines: HashMap<String, Counter>,
}

fn bytes(text: &str) -> Option<u64> {
    let mut fields = text.split_whitespace();
    let amount: u64 = fields.next()?.parse().ok()?;
    let scale = match fields.next() {
        None | Some("B") => 1,
        Some("KiB" | "kB") => 1024,
        Some("MiB") => 1024 * 1024,
        _ => return None,
    };
    amount
        .checked_mul(scale)
        .filter(|_| fields.next().is_none())
}

fn parse(text: &str, driver: Driver, pci: &str) -> Option<Client> {
    let mut id = None;
    let mut matched = false;
    let mut has_memory = false;
    let mut memory = HashMap::new();
    let mut engines = HashMap::<String, Counter>::new();
    for (key, value) in text.lines().filter_map(|line| line.split_once(':')) {
        let value = value.trim();
        match key {
            "drm-driver" => {
                if Driver::parse(value) != Some(driver) {
                    return None;
                }
                matched = true;
            }
            "drm-pdev" if !value.eq_ignore_ascii_case(pci) => return None,
            "drm-client-id" => id = value.parse().ok(),
            _ => {
                if let Some(region) = key.strip_prefix("drm-resident-") {
                    let amount = bytes(value);
                    memory.insert(region, amount);
                    has_memory |= amount.is_some();
                    continue;
                }
                if driver == Driver::Amd
                    && let Some(region) = key.strip_prefix("drm-memory-")
                {
                    // The AMD legacy alias is rounded to KiB. Prefer the resident counter.
                    let amount = bytes(value);
                    memory.entry(region).or_insert(amount);
                    has_memory |= amount.is_some();
                    continue;
                }
                if let Some(engine) = key
                    .strip_prefix("drm-engine-capacity-")
                    .filter(|e| driver.engine(e))
                {
                    engines.entry(engine.into()).or_default().capacity =
                        Some(value.parse().unwrap_or(0));
                } else if let Some(engine) = key
                    .strip_prefix("drm-engine-")
                    .filter(|e| driver != Driver::Xe && driver.engine(e))
                {
                    let time = value
                        .strip_suffix(" ns")
                        .and_then(|v| v.trim().parse().ok());
                    engines.entry(engine.into()).or_default().busy = time;
                } else if let Some(engine) = key
                    .strip_prefix("drm-cycles-")
                    .filter(|e| driver == Driver::Xe && driver.engine(e))
                {
                    let busy = value.parse().ok();
                    engines.entry(engine.into()).or_default().busy = busy;
                } else if let Some(engine) = key
                    .strip_prefix("drm-total-cycles-")
                    .filter(|e| driver == Driver::Xe && driver.engine(e))
                {
                    engines.entry(engine.into()).or_default().total = value.parse().ok();
                }
            }
        }
    }
    let has_engine = engines.values().any(|counter| {
        counter.busy.is_some()
            && counter.capacity != Some(0)
            && (driver != Driver::Xe || counter.total.is_some())
    });
    if !matched || !(has_memory || has_engine) {
        return None;
    }
    if driver == Driver::Amd {
        // amdgpu omits the engine-time field until the first work on that engine.
        for engine in ["gfx", "compute"] {
            engines.entry(engine.into()).or_insert_with(|| Counter {
                busy: Some(0),
                ..Default::default()
            });
        }
    }
    let sum = |local_only: bool| {
        let mut regions = memory
            .iter()
            .filter(|(region, _)| {
                !local_only
                    || matches!(
                        region.trim_end_matches(|c: char| c.is_ascii_digit()),
                        "local" | "vram"
                    )
            })
            .peekable();
        regions.peek()?;
        regions.try_fold(0_u64, |total, (_, bytes)| total.checked_add((*bytes)?))
    };
    Some(Client {
        id: id?,
        bytes: sum(false),
        dedicated: sum(true),
        engines,
    })
}

type Identity = (u32, u128, u64);

struct Measurement {
    at: Instant,
    engines: HashMap<String, Counter>,
}

impl Measurement {
    fn usage(&mut self, previous: &Self, driver: Driver) -> Option<f64> {
        if self.engines.is_empty() {
            return None;
        }
        let elapsed = self.at.checked_duration_since(previous.at)?.as_secs_f64() * 1e9;
        let mut usage = previous
            .engines
            .keys()
            .all(|name| self.engines.contains_key(name))
            .then_some(0.0);
        for (name, current) in &mut self.engines {
            let delta = previous.engines.get(name).and_then(|old| {
                // Some DRM drivers temporarily report lower counters. Retain
                // each high-water mark until the driver catches up.
                let difference = |current: &mut Option<u64>, old: Option<u64>| {
                    let (new, old) = ((*current)?, old?);
                    *current = Some(new.max(old));
                    new.checked_sub(old)
                };
                let busy = difference(&mut current.busy, old.busy);
                let interval = if driver == Driver::Xe {
                    difference(&mut current.total, old.total)? as f64
                } else {
                    elapsed
                };
                let capacity = current.capacity.unwrap_or(1);
                if capacity == 0 || capacity != old.capacity.unwrap_or(1) || interval <= 0.0 {
                    return None;
                }
                Some(100.0 * busy? as f64 / (interval * capacity as f64))
            });
            usage = usage.zip(delta).map(|(a, b)| a + b);
        }
        usage
    }
}

pub(super) struct Device {
    pci: String,
    driver: Driver,
    nodes: Vec<(PathBuf, u64)>,
    process_only: bool,
    supported: bool,
    retry: Option<Instant>,
    previous: HashMap<Identity, Measurement>,
    kfd: Option<Kfd>,
}

struct Kfd {
    id: u64,
    units: u64,
}

impl Kfd {
    fn discover(render_minor: u64, root: &Path) -> Option<Self> {
        fs::read_dir(root).ok()?.flatten().find_map(|entry| {
            let path = entry.path();
            let properties = fs::read_to_string(path.join("properties")).ok()?;
            let field = |name| {
                properties
                    .lines()
                    .filter_map(|s| s.split_once(' '))
                    .find(|(key, _)| *key == name)?
                    .1
                    .trim()
                    .parse::<u64>()
                    .ok()
            };
            if field("drm_render_minor")? != render_minor {
                return None;
            }
            let units = field("simd_count")?.checked_div(field("simd_per_cu")?)?;
            let id = super::read_trimmed(path.join("gpu_id"))?.parse().ok()?;
            (units > 0 && id != 0).then_some(Self { id, units })
        })
    }

    fn occupancy(&self, root: &Path, pid: u32) -> Option<u32> {
        let path = root.join(format!("{pid}/stats_{}/cu_occupancy", self.id));
        let active: u64 = super::read_trimmed(path)?.parse().ok()?;
        Some(
            (100.0 * active as f64 / self.units as f64)
                .round()
                .min(100.0) as u32,
        )
    }
}

#[derive(Default)]
pub(super) struct RsmiDevice {
    pub sysfs_path: Option<PathBuf>,
    pub drm: Option<Device>,
}

pub(super) fn rsmi_devices(library: &super::DynamicLibrary, count: u32) -> Vec<RsmiDevice> {
    type PciId = unsafe extern "C" fn(u32, *mut u64) -> super::RsmiStatus;
    let render_minor = library.symbol::<super::RsmiU32>(b"rsmi_dev_drm_render_minor_get\0");
    let pci_id = library.symbol::<PciId>(b"rsmi_dev_pci_id_get\0");
    let paths: Vec<_> = (0..count)
        .map(|index| {
            let mut minor = 0;
            if render_minor
                .is_some_and(|get| unsafe { get(index, &mut minor) } == super::RSMI_SUCCESS)
                && let Ok(path) = fs::canonicalize(format!("/sys/class/drm/renderD{minor}/device"))
            {
                return Some(path);
            }
            let mut id = 0;
            if unsafe { pci_id?(index, &mut id) } != super::RSMI_SUCCESS {
                return None;
            }
            let (domain, bus, slot, function) =
                (id >> 32, (id >> 8) & 0xff, (id >> 3) & 0x1f, id & 7);
            fs::canonicalize(format!(
                "/sys/bus/pci/devices/{domain:04x}:{bus:02x}:{slot:02x}.{function:x}"
            ))
            .ok()
        })
        .collect();
    paths
        .iter()
        .map(|path| {
            let drm = path.as_ref().and_then(|path| {
                // A DRM client cannot identify an RSMI partition sharing a physical device.
                if paths.iter().flatten().filter(|p| *p == path).count() != 1 {
                    return None;
                }
                Device::discover(path, false).filter(|device| device.driver == Driver::Amd)
            });
            RsmiDevice {
                sysfs_path: path.clone(),
                drm,
            }
        })
        .collect()
}

fn sysfs_device_number(node: &Path) -> Option<u64> {
    let dev = super::read_trimmed(node.join("dev"))?;
    let (major, minor) = dev.split_once(':')?;
    let major: u64 = major.parse().ok()?;
    let minor: u64 = minor.parse().ok()?;
    // Linux new_encode_dev splits the 20-bit minor around the 12-bit major.
    (major <= 0xfff && minor <= 0xfffff)
        .then_some((major << 8) | (minor & 0xff) | ((minor & 0xfff00) << 12))
}

impl Device {
    pub(super) fn discover(path: &Path, process_only: bool) -> Option<Self> {
        Self::discover_at(
            path,
            Path::new("/dev/dri"),
            Path::new("/sys/class/kfd/kfd/topology/nodes"),
            process_only,
        )
    }

    fn discover_at(
        path: &Path,
        dev_dri: &Path,
        topology: &Path,
        process_only: bool,
    ) -> Option<Self> {
        let pci = fs::canonicalize(path)
            .ok()?
            .file_name()?
            .to_str()?
            .to_owned();
        let driver = Driver::parse(
            fs::read_link(path.join("driver"))
                .ok()?
                .file_name()?
                .to_str()?,
        )?;
        let mut nodes = Vec::new();
        let mut kfd = None;
        for entry in fs::read_dir(path.join("drm")).ok()?.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let minor = name.strip_prefix("renderD").and_then(|s| s.parse().ok());
            if super::drm_card_index(name).is_none() && minor.is_none() {
                continue;
            }
            let Some(rdev) = sysfs_device_number(&entry.path()) else {
                continue;
            };
            if driver == Driver::Amd
                && let Some(minor) = minor
            {
                kfd = Kfd::discover(minor, topology);
            }
            nodes.push((dev_dri.join(name), rdev));
        }
        // Prefer a render node for the capability probe on an idle GPU.
        nodes.sort_by_key(|(path, _)| {
            path.file_name()
                .is_none_or(|s| !s.as_encoded_bytes().starts_with(b"renderD"))
        });
        (!nodes.is_empty()).then_some(Self {
            pci,
            driver,
            nodes,
            kfd,
            previous: HashMap::new(),
            process_only,
            supported: false,
            retry: None,
        })
    }

    pub(super) fn awaiting_telemetry(&self) -> bool {
        self.process_only && !self.supported
    }

    fn probe(&self, procfs: &Path) -> bool {
        self.nodes.iter().any(|(node, rdev)| {
            let matches =
                |stat: fs::Metadata| stat.file_type().is_char_device() && stat.rdev() == *rdev;
            if !fs::metadata(node).is_ok_and(matches) {
                return false;
            }
            let Ok(file) = fs::File::open(node) else {
                return false;
            };
            if !file.metadata().is_ok_and(matches) {
                return false;
            }
            fs::read_to_string(procfs.join(format!("self/fdinfo/{}", file.as_raw_fd())))
                .ok()
                .and_then(|text| parse(&text, self.driver, &self.pci))
                .is_some()
        })
    }
}

struct Scan<'a> {
    index: usize,
    device: &'a mut Device,
    snapshot: Snapshot,
    utilization: HashMap<u32, Option<f64>>,
    current: HashMap<Identity, Measurement>,
}

impl Scan<'_> {
    fn record(&mut self, process: &ProcessSample, client: Client, now: Instant) {
        let Some(start) = process.start_time else {
            return;
        };
        let key = (process.pid, start, client.id);
        let std::collections::hash_map::Entry::Vacant(slot) = self.current.entry(key) else {
            return;
        };
        let mut measurement = Measurement {
            at: now,
            engines: client.engines,
        };
        let usage = self
            .device
            .previous
            .get(&key)
            .and_then(|old| measurement.usage(old, self.device.driver));
        slot.insert(measurement);
        let pid = process.pid;
        let sum = |table: &mut HashMap<u32, Option<u64>>, value| {
            let entry = table.entry(pid).or_insert(Some(0));
            *entry = entry.zip(value).and_then(|(a, b)| a.checked_add(b));
        };
        sum(&mut self.snapshot.processes, client.bytes);
        sum(
            self.snapshot.dedicated.get_or_insert_default(),
            client.dedicated,
        );
        let total = self.utilization.entry(pid).or_insert(Some(0.0));
        *total = total.zip(usage).map(|(a, b)| a + b);
        self.device.supported = true;
    }

    fn finish(mut self, process_count: usize, kfd_proc: &Path) -> Snapshot {
        self.snapshot.available = process_count == 0
            || self.snapshot.unreadable.len() < process_count
            || !self.current.is_empty();
        self.snapshot.utilization = Some(
            self.utilization
                .into_iter()
                .map(|(pid, usage)| {
                    let usage = usage.map(|usage| {
                        let occupancy = self
                            .device
                            .kfd
                            .as_ref()
                            .and_then(|kfd| kfd.occupancy(kfd_proc, pid))
                            .unwrap_or(0);
                        (usage + f64::from(occupancy)).round().min(100.0) as u32
                    });
                    (pid, usage)
                })
                .collect(),
        );
        for pid in &self.snapshot.unreadable {
            if let Some(bytes) = self.snapshot.processes.get_mut(pid) {
                *bytes = None;
                self.snapshot
                    .utilization
                    .get_or_insert_default()
                    .insert(*pid, None);
                self.snapshot
                    .dedicated
                    .get_or_insert_default()
                    .insert(*pid, None);
            }
        }
        self.device.previous = self.current;
        self.snapshot
    }
}

pub(super) fn collect(
    devices: &mut [(usize, &mut Device)],
    samples: &mut [GpuSample],
    processes: &[ProcessSample],
    requested: bool,
    root: &Path,
    now: Instant,
) {
    let mut scans = Vec::new();
    for (index, device) in devices {
        let index = *index;
        let probe = !device.supported && device.retry.is_none_or(|deadline| now >= deadline);
        if probe {
            device.retry = Some(now + Duration::from_secs(10));
            device.supported = device.probe(root);
        }
        samples[index].support.processes = device.supported;
        if !requested {
            device.previous.clear();
        }
        if (requested && device.supported) || (probe && !device.supported) {
            scans.push(Scan {
                index,
                device,
                snapshot: Snapshot {
                    complete: true,
                    dedicated: Some(HashMap::new()),
                    ..Default::default()
                },
                utilization: HashMap::new(),
                current: HashMap::new(),
            });
        }
    }
    if scans.is_empty() {
        return;
    }
    let by_node: HashMap<u64, usize> = scans
        .iter()
        .enumerate()
        .flat_map(|(i, scan)| scan.device.nodes.iter().map(move |(_, node)| (*node, i)))
        .collect();
    for process in processes {
        let dir = root.join(process.pid.to_string());
        let Ok(fds) = fs::read_dir(dir.join("fd")) else {
            for scan in &mut scans {
                scan.snapshot.unreadable.insert(process.pid);
            }
            continue;
        };
        for entry in fds {
            let result = entry.and_then(|fd| Ok((fs::metadata(fd.path())?, fd.file_name())));
            let (stat, name) = match result {
                Ok(pair) => pair,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => {
                    for scan in &mut scans {
                        scan.snapshot.unreadable.insert(process.pid);
                    }
                    continue;
                }
            };
            if !stat.file_type().is_char_device() {
                continue;
            }
            let Some(&index) = by_node.get(&stat.rdev()) else {
                continue;
            };
            let scan = &mut scans[index];
            let client = fs::read_to_string(dir.join("fdinfo").join(name))
                .ok()
                .and_then(|text| parse(&text, scan.device.driver, &scan.device.pci));
            if let Some(client) = client {
                scan.record(process, client, now);
            } else {
                scan.snapshot.unreadable.insert(process.pid);
            }
        }
    }
    for scan in scans {
        samples[scan.index].support.processes = scan.device.supported;
        if requested && scan.device.supported {
            let index = scan.index;
            samples[index].process_usage =
                Some(scan.finish(processes.len(), Path::new("/sys/class/kfd/kfd/proc")));
        }
    }
}

#[cfg(test)]
mod tests;
