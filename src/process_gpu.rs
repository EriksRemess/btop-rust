use std::collections::{HashMap, HashSet};

use crate::collect::ProcessSample;
use crate::gpu::GpuSample;

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub available: bool,
    pub complete: bool,
    pub unreadable: HashSet<u32>,
    // DRM can report both system and device-local resident memory.
    // NVML reports only device memory and uses the primary byte count.
    pub dedicated: Option<HashMap<u32, Option<u64>>>,
    pub processes: HashMap<u32, Option<u64>>,
    pub utilization: Option<HashMap<u32, Option<u32>>>,
}

pub fn available(gpus: &[GpuSample]) -> bool {
    gpus.iter().any(|gpu| gpu.support.processes)
}

#[derive(Debug, Clone, Default)]
pub struct Usage {
    pub dedicated_bytes: Option<u64>,
    pub bytes: Option<u64>,
    pub utilization: Option<u32>,
    pub devices: Vec<DeviceUsage>,
}

#[derive(Debug, Clone)]
pub struct DeviceUsage {
    pub index: usize,
    pub name: String,
    pub bytes: Option<u64>,
    pub utilization: Option<u32>,
}

pub fn attach(
    processes: &mut [ProcessSample],
    gpus: &[GpuSample],
    mut current_start: impl FnMut(u32) -> Option<u128>,
) {
    let devices: Vec<_> = gpus
        .iter()
        .enumerate()
        .filter_map(|(index, gpu)| gpu.process_usage.as_ref().map(|data| (index, gpu, data)))
        .collect();
    let complete = !devices.is_empty() && devices.iter().all(|(_, _, data)| data.complete);
    for process in processes {
        let mut memory = Usage::default();
        for (index, gpu, data) in &devices {
            if let Some(bytes) = data.processes.get(&process.pid) {
                memory.devices.push(DeviceUsage {
                    index: *index,
                    name: gpu.name.clone(),
                    bytes: *bytes,
                    utilization: data
                        .utilization
                        .as_ref()
                        .and_then(|values| values.get(&process.pid).copied().unwrap_or(Some(0))),
                });
            }
        }
        // Driver data is keyed by PID. Confirm identity after sampling.
        if !memory.devices.is_empty()
            && (process.start_time.is_none() || current_start(process.pid) != process.start_time)
        {
            process.gpu = Usage::default();
            continue;
        }
        let readable = devices
            .iter()
            .all(|(_, _, data)| !data.unreadable.contains(&process.pid));
        if readable && complete {
            memory.dedicated_bytes = devices.iter().try_fold(0_u64, |total, (_, _, data)| {
                let bytes = data.dedicated.as_ref().unwrap_or(&data.processes);
                total.checked_add(bytes.get(&process.pid).copied().unwrap_or(Some(0))?)
            });
            memory.bytes = memory
                .devices
                .iter()
                .try_fold(0_u64, |total, device| total.checked_add(device.bytes?));
        }
        if readable
            && !devices.is_empty()
            && devices
                .iter()
                .all(|(_, _, data)| data.complete || data.utilization.is_some())
        {
            memory.utilization = memory.devices.iter().try_fold(0_u32, |total, device| {
                total.checked_add(device.utilization?)
            });
        }
        process.gpu = memory;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gpu(entries: &[(u32, Option<u64>)], complete: bool) -> GpuSample {
        GpuSample {
            name: "test GPU".into(),
            process_usage: Some(Snapshot {
                complete,
                processes: entries.iter().copied().collect(),
                ..Snapshot::default()
            }),
            ..GpuSample::default()
        }
    }

    fn process(pid: u32) -> ProcessSample {
        ProcessSample {
            pid,
            start_time: Some(10),
            ..ProcessSample::default()
        }
    }

    #[test]
    fn multi_gpu_totals_distinguish_zero_and_unavailable() {
        let mut processes = vec![process(1), process(2), process(3)];
        let devices = vec![
            gpu(&[(1, Some(100)), (2, None)], true),
            gpu(&[(1, Some(50))], true),
            GpuSample::default(),
        ];
        attach(&mut processes, &devices, |_| Some(10));
        assert_eq!(processes[0].gpu.bytes, Some(150));
        assert_eq!(processes[0].gpu.devices.len(), 2);
        assert_eq!(processes[0].gpu.devices[1].index, 1);
        assert_eq!(processes[1].gpu.bytes, None);
        assert_eq!(processes[1].gpu.devices.len(), 1);
        assert_eq!(processes[2].gpu.bytes, Some(0));
        assert!(processes[2].gpu.devices.is_empty());
        attach(&mut processes, &[gpu(&[(1, Some(200))], false)], |_| {
            Some(10)
        });
        assert!(processes.iter().all(|process| process.gpu.bytes.is_none()));
        assert_eq!(processes[0].gpu.devices[0].bytes, Some(200));
        attach(&mut processes, &[], |_| Some(10));
        assert!(
            processes
                .iter()
                .all(|process| process.gpu.bytes.is_none() && process.gpu.devices.is_empty())
        );
    }

    #[test]
    fn exited_or_reused_pids_cannot_receive_gpu_memory() {
        for current in [None, Some(20)] {
            let mut processes = vec![process(1)];
            attach(&mut processes, &[gpu(&[(1, Some(100))], true)], |_| current);
            assert!(processes[0].gpu.bytes.is_none());
            assert!(processes[0].gpu.devices.is_empty());
        }
        let mut processes = vec![process(1)];
        processes[0].start_time = None;
        attach(&mut processes, &[gpu(&[(1, Some(100))], true)], |_| None);
        assert!(processes[0].gpu.devices.is_empty());
    }

    #[test]
    fn utilization_sums_devices_and_distinguishes_idle_from_unsupported() {
        let mut processes = vec![process(1), process(2)];
        let mut first = gpu(&[(1, Some(100))], true);
        first.process_usage.as_mut().unwrap().utilization = Some(HashMap::from([(1, Some(60))]));
        let mut second = gpu(&[(1, Some(50))], true);
        second.process_usage.as_mut().unwrap().utilization = Some(HashMap::from([(1, Some(70))]));
        attach(&mut processes, &[first.clone(), second.clone()], |_| {
            Some(10)
        });
        assert_eq!(processes[0].gpu.utilization, Some(130));
        assert_eq!(processes[1].gpu.utilization, Some(0));
        second.process_usage.as_mut().unwrap().utilization = None;
        attach(&mut processes, &[first, second], |_| Some(10));
        assert_eq!(processes[0].gpu.utilization, None);
        assert_eq!(processes[0].gpu.devices[0].utilization, Some(60));
    }

    #[test]
    fn unrelated_memory_only_gpu_does_not_hide_measured_utilization() {
        let mut processes = [process(1), process(2)];
        let mut measured = gpu(&[(1, Some(100))], true);
        measured.process_usage.as_mut().unwrap().utilization = Some(HashMap::from([(1, Some(35))]));
        let mut memory_only = gpu(&[(2, Some(200))], true);
        attach(
            &mut processes,
            &[measured.clone(), memory_only.clone()],
            |_| Some(10),
        );
        assert_eq!(processes[0].gpu.utilization, Some(35));
        assert_eq!(processes[1].gpu.utilization, None);
        memory_only.process_usage.as_mut().unwrap().complete = false;
        attach(&mut processes, &[measured, memory_only], |_| Some(10));
        assert_eq!(processes[0].gpu.utilization, None);
    }

    #[test]
    fn overflowing_totals_remain_unavailable() {
        let mut processes = vec![process(1)];
        attach(
            &mut processes,
            &[
                gpu(&[(1, Some(u64::MAX - 1))], true),
                gpu(&[(1, Some(2))], true),
            ],
            |_| Some(10),
        );
        assert_eq!(processes[0].gpu.bytes, None);
    }
}
