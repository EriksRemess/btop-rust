use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{DynamicLibrary, NvmlDevice, NvmlReturn, c_uint};
use crate::process_gpu::Snapshot;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessInfo {
    pid: c_uint,
    memory: u64,
    gpu_instance: c_uint,
    compute_instance: c_uint,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LegacyProcessInfo {
    pid: c_uint,
    memory: u64,
}

type Query = unsafe extern "C" fn(NvmlDevice, *mut c_uint, *mut ProcessInfo) -> NvmlReturn;
type LegacyQuery =
    unsafe extern "C" fn(NvmlDevice, *mut c_uint, *mut LegacyProcessInfo) -> NvmlReturn;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct UtilizationSample {
    pid: c_uint,
    timestamp: u64,
    sm: c_uint,
    memory: c_uint,
    encoder: c_uint,
    decoder: c_uint,
}

type UtilizationQuery =
    unsafe extern "C" fn(NvmlDevice, *mut UtilizationSample, *mut c_uint, u64) -> NvmlReturn;

enum ProcessQuery {
    Current(Query),
    Legacy(LegacyQuery),
}

impl ProcessQuery {
    fn load(library: &DynamicLibrary, prefix: &str) -> Option<Self> {
        for suffix in ["_v3", "_v2"] {
            if let Some(query) = library.symbol(format!("{prefix}{suffix}\0").as_bytes()) {
                return Some(Self::Current(query));
            }
        }
        library
            .symbol(format!("{prefix}\0").as_bytes())
            .map(Self::Legacy)
    }

    fn read(&self, device: NvmlDevice) -> Option<Vec<ProcessInfo>> {
        match self {
            Self::Current(query) => {
                read_processes(|count, entries| unsafe { query(device, count, entries) })
            }
            Self::Legacy(query) => {
                read_processes(|count, entries| unsafe { query(device, count, entries) }).map(
                    |entries| {
                        entries
                            .into_iter()
                            .map(|entry| ProcessInfo {
                                pid: entry.pid,
                                memory: entry.memory,
                                gpu_instance: u32::MAX,
                                compute_instance: u32::MAX,
                            })
                            .collect()
                    },
                )
            }
        }
    }
}

#[derive(Default)]
pub(super) struct Functions {
    graphics: Option<ProcessQuery>,
    compute: Option<ProcessQuery>,
    utilization: Option<UtilizationQuery>,
}

impl Functions {
    pub(super) fn load(library: &DynamicLibrary) -> Self {
        Self {
            graphics: ProcessQuery::load(library, "nvmlDeviceGetGraphicsRunningProcesses"),
            compute: ProcessQuery::load(library, "nvmlDeviceGetComputeRunningProcesses"),
            utilization: library.symbol(b"nvmlDeviceGetProcessUtilization\0"),
        }
    }

    pub(super) fn collect(&self, device: NvmlDevice) -> Snapshot {
        let mut snapshot = merge(
            self.graphics.as_ref().and_then(|query| query.read(device)),
            self.compute.as_ref().and_then(|query| query.read(device)),
        );
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|time| u64::try_from(time.as_micros()).ok());
        if let Some((query, now)) = self.utilization.zip(now) {
            snapshot.utilization = read_utilization(query, device, now);
            if let Some(utilization) = &snapshot.utilization {
                for pid in utilization.keys() {
                    snapshot.processes.entry(*pid).or_insert(None);
                }
            }
        }
        snapshot.available |= snapshot.utilization.is_some();
        snapshot
    }
}

fn read_utilization(
    query: UtilizationQuery,
    device: NvmlDevice,
    now: u64,
) -> Option<HashMap<u32, Option<u32>>> {
    // NVML retains older nonzero samples. Query only the latest one-second
    // sampling window so an idle process cannot keep an old busy percentage.
    let since = now.saturating_sub(1_000_000);
    let samples = read_processes(|count, entries| unsafe {
        let result = query(device, entries, count, since);
        if result == 6 {
            // NVML_ERROR_NOT_FOUND: no activity in this window.
            *count = 0;
            0
        } else {
            result
        }
    })?;
    let mut latest = HashMap::new();
    for sample in samples {
        if sample.timestamp <= since || sample.timestamp > now {
            continue;
        }
        let value = latest.entry(sample.pid).or_insert((0, None));
        if sample.timestamp >= value.0 {
            *value = (sample.timestamp, (sample.sm <= 100).then_some(sample.sm));
        }
    }
    Some(
        latest
            .into_iter()
            .map(|(pid, (_, value))| (pid, value))
            .collect(),
    )
}

fn read_processes<T: Default + Clone>(
    mut query: impl FnMut(*mut c_uint, *mut T) -> NvmlReturn,
) -> Option<Vec<T>> {
    const INSUFFICIENT_SIZE: NvmlReturn = 7;
    let mut capacity = 16usize;
    for _ in 0..4 {
        if capacity > 65_536 {
            return None;
        }
        let mut entries = vec![T::default(); capacity];
        let mut count = capacity as c_uint;
        match query(&mut count, entries.as_mut_ptr()) {
            0 if count as usize <= capacity => {
                entries.truncate(count as usize);
                return Some(entries);
            }
            INSUFFICIENT_SIZE => capacity = (count as usize).max(capacity * 2).saturating_add(8),
            _ => return None,
        }
    }
    None
}

fn merge(graphics: Option<Vec<ProcessInfo>>, compute: Option<Vec<ProcessInfo>>) -> Snapshot {
    let available = graphics.is_some() || compute.is_some();
    let complete = graphics.is_some() && compute.is_some();
    let mut instances: HashMap<(u32, u32, u32), Option<u64>> = HashMap::new();
    for entry in graphics.into_iter().chain(compute).flatten() {
        let bytes = (entry.memory != u64::MAX).then_some(entry.memory);
        // Both APIs report the application's memory, not separate graphics
        // and compute allocations. Samples can differ slightly between calls.
        instances
            .entry((entry.pid, entry.gpu_instance, entry.compute_instance))
            .and_modify(|previous| *previous = (*previous).max(bytes))
            .or_insert(bytes);
    }
    let mut processes: HashMap<u32, Option<u64>> = HashMap::new();
    for ((pid, _, _), bytes) in instances {
        let total = processes.entry(pid).or_insert(Some(0));
        *total = total
            .zip(bytes)
            .and_then(|(total, bytes)| total.checked_add(bytes));
    }
    Snapshot {
        available,
        complete,
        processes,
        utilization: None,
    }
}

#[derive(Default)]
pub(super) struct DeviceState {
    supported: bool,
    retry_at: Option<std::time::Instant>,
}

impl DeviceState {
    pub(super) fn collect(
        &mut self,
        functions: &Functions,
        device: NvmlDevice,
        requested: bool,
        now: std::time::Instant,
    ) -> (bool, Option<Snapshot>) {
        let probe_due = !self.supported && self.retry_at.is_none_or(|retry| now >= retry);
        let snapshot = if probe_due || requested && self.supported {
            self.retry_at = Some(now + std::time::Duration::from_secs(10));
            let snapshot = functions.collect(device);
            // Support is a device capability; a failed sample must not make
            // an already working panel disappear.
            self.supported |= snapshot.available;
            (requested && self.supported).then_some(snapshot)
        } else {
            None
        };
        (self.supported, snapshot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_probe_handles_missing_apis_idle_devices_retries_and_sample_failures() {
        use std::cell::Cell;
        use std::time::{Duration, Instant};

        struct Driver {
            calls: Cell<usize>,
            status: Cell<NvmlReturn>,
        }
        unsafe extern "C" fn query(
            device: NvmlDevice,
            count: *mut c_uint,
            _: *mut ProcessInfo,
        ) -> NvmlReturn {
            let driver = unsafe { &*device.cast::<Driver>() };
            driver.calls.set(driver.calls.get() + 1);
            unsafe {
                *count = 0;
            }
            driver.status.get()
        }
        let now = Instant::now();
        let mut state = DeviceState::default();
        let (supported, sample) =
            state.collect(&Functions::default(), std::ptr::null_mut(), true, now);
        assert!(!supported && sample.is_none());

        let driver = Driver {
            calls: Cell::new(0),
            status: Cell::new(3),
        };
        let device = (&driver as *const Driver).cast_mut().cast();
        let functions = Functions {
            graphics: Some(ProcessQuery::Current(query)),
            ..Default::default()
        };
        let mut state = DeviceState::default();
        assert!(!state.collect(&functions, device, false, now).0);
        driver.status.set(0);
        assert!(
            !state
                .collect(&functions, device, true, now + Duration::from_secs(9))
                .0
        );
        assert_eq!(driver.calls.get(), 1);
        let (supported, sample) =
            state.collect(&functions, device, false, now + Duration::from_secs(10));
        assert!(supported && sample.is_none());
        assert_eq!(driver.calls.get(), 2);
        assert!(
            state
                .collect(&functions, device, false, now + Duration::from_secs(20))
                .0
        );
        assert_eq!(
            driver.calls.get(),
            2,
            "hidden panels do not keep querying supported devices"
        );
        let sample = state
            .collect(&functions, device, true, now + Duration::from_secs(21))
            .1
            .unwrap();
        assert!(sample.available && sample.processes.is_empty());
        assert!(
            !sample.complete,
            "one working API is sufficient for capability discovery"
        );
        driver.status.set(999);
        let (supported, sample) =
            state.collect(&functions, device, true, now + Duration::from_secs(22));
        assert!(
            supported,
            "a failed sample must not hide a supported feature"
        );
        let sample = sample.unwrap();
        assert!(!sample.available && !sample.complete && sample.processes.is_empty());
    }

    fn entry(pid: u32, memory: u64) -> ProcessInfo {
        ProcessInfo {
            pid,
            memory,
            gpu_instance: u32::MAX,
            compute_instance: u32::MAX,
        }
    }

    #[test]
    fn graphics_and_compute_memory_is_counted_once() {
        let snapshot = merge(
            Some(vec![entry(1, 100), entry(2, 0), entry(3, u64::MAX)]),
            Some(vec![entry(1, 110), entry(3, 30), entry(4, u64::MAX)]),
        );
        assert!(snapshot.complete);
        assert_eq!(
            snapshot.processes,
            HashMap::from([(1, Some(110)), (2, Some(0)), (3, Some(30)), (4, None)])
        );
        let mut instance = entry(1, 50);
        instance.gpu_instance = 2;
        instance.compute_instance = 0;
        let snapshot = merge(Some(vec![entry(1, 100), instance]), Some(vec![instance]));
        assert_eq!(snapshot.processes[&1], Some(150));
    }

    #[test]
    fn failed_queries_do_not_invent_zeroes_or_keep_previous_values() {
        let snapshot = merge(Some(vec![entry(1, 100)]), None);
        assert!(!snapshot.complete);
        assert_eq!(snapshot.processes[&1], Some(100));
        let failed = merge(None, None);
        assert!(!failed.complete);
        assert!(failed.processes.is_empty());
        assert!(merge(Some(vec![]), Some(vec![])).complete);
        assert!(!Functions::default().collect(std::ptr::null_mut()).complete);
    }

    #[test]
    fn process_queries_retry_growth_and_bound_driver_counts() {
        let mut calls = 0;
        let entries = read_processes::<ProcessInfo>(|count, entries| unsafe {
            calls += 1;
            if calls < 3 {
                *count += 20;
                7
            } else {
                *entries = entry(42, 123);
                *count = 1;
                0
            }
        })
        .unwrap();
        assert_eq!(calls, 3);
        assert_eq!((entries[0].pid, entries[0].memory), (42, 123));
        assert!(read_processes::<ProcessInfo>(|_, _| 7).is_none());
        assert!(read_processes::<ProcessInfo>(|_, _| 3).is_none());
        for result in [0, 7] {
            assert!(
                read_processes::<ProcessInfo>(|count, _| unsafe {
                    *count = u32::MAX;
                    result
                })
                .is_none()
            );
        }
    }

    #[test]
    fn utilization_uses_latest_fresh_samples_and_preserves_unavailability() {
        unsafe extern "C" fn samples(
            _: NvmlDevice,
            entries: *mut UtilizationSample,
            count: *mut c_uint,
            since: u64,
        ) -> NvmlReturn {
            assert_eq!(since, 1_000_000);
            let values = [
                (1, 1_900_000, 40),
                (1, 1_800_000, 90),
                (2, 900_000, 80),
                (3, 1_900_000, 101),
            ];
            for (index, (pid, timestamp, sm)) in values.into_iter().enumerate() {
                unsafe {
                    *entries.add(index) = UtilizationSample {
                        pid,
                        timestamp,
                        sm,
                        ..UtilizationSample::default()
                    };
                }
            }
            unsafe {
                *count = values.len() as c_uint;
            }
            0
        }
        unsafe extern "C" fn idle(
            _: NvmlDevice,
            _: *mut UtilizationSample,
            _: *mut c_uint,
            _: u64,
        ) -> NvmlReturn {
            6
        }
        unsafe extern "C" fn unsupported(
            _: NvmlDevice,
            _: *mut UtilizationSample,
            _: *mut c_uint,
            _: u64,
        ) -> NvmlReturn {
            3
        }
        let device = std::ptr::null_mut();
        assert_eq!(
            read_utilization(samples, device, 2_000_000),
            Some(HashMap::from([(1, Some(40)), (3, None)]))
        );
        assert_eq!(
            read_utilization(idle, device, 2_000_000),
            Some(HashMap::new())
        );
        assert_eq!(read_utilization(unsupported, device, 2_000_000), None);
    }

    #[test]
    fn legacy_query_uses_legacy_record_stride() {
        unsafe extern "C" fn query(
            _: NvmlDevice,
            count: *mut c_uint,
            entries: *mut LegacyProcessInfo,
        ) -> NvmlReturn {
            unsafe {
                *entries = LegacyProcessInfo {
                    pid: 1,
                    memory: 100,
                };
                *entries.add(1) = LegacyProcessInfo {
                    pid: 2,
                    memory: 200,
                };
                *count = 2;
            }
            0
        }
        let entries = ProcessQuery::Legacy(query)
            .read(std::ptr::null_mut())
            .unwrap();
        assert_eq!(
            entries
                .iter()
                .map(|entry| (entry.pid, entry.memory))
                .collect::<Vec<_>>(),
            [(1, 100), (2, 200)]
        );
        assert!(entries.iter().all(|entry| entry.gpu_instance == u32::MAX));
    }
}
