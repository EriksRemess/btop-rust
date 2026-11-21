use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

unsafe fn available(device: NvmlDevice) -> bool {
    unsafe { &*device.cast::<AtomicBool>() }.load(Ordering::Relaxed)
}

unsafe extern "C" fn shutdown() -> NvmlReturn {
    NVML_SUCCESS
}

unsafe extern "C" fn name(_: NvmlDevice, _: *mut c_char, _: c_uint) -> NvmlReturn {
    1
}

unsafe extern "C" fn uint(device: NvmlDevice, output: *mut c_uint) -> NvmlReturn {
    if unsafe { available(device) } {
        unsafe { *output = 42 };
        NVML_SUCCESS
    } else {
        1
    }
}

unsafe extern "C" fn kind_uint(device: NvmlDevice, _: c_int, output: *mut c_uint) -> NvmlReturn {
    unsafe { uint(device, output) }
}

unsafe extern "C" fn utilization(device: NvmlDevice, output: *mut NvmlUtilization) -> NvmlReturn {
    if unsafe { available(device) } {
        unsafe {
            *output = NvmlUtilization {
                gpu: 42,
                memory: 21,
            }
        };
        NVML_SUCCESS
    } else {
        1
    }
}

unsafe extern "C" fn memory(device: NvmlDevice, output: *mut NvmlMemory) -> NvmlReturn {
    if unsafe { available(device) } {
        unsafe {
            *output = NvmlMemory {
                total: 100,
                free: 60,
                used: 40,
            }
        };
        NVML_SUCCESS
    } else {
        1
    }
}

unsafe extern "C" fn codec(device: NvmlDevice, output: *mut c_uint, _: *mut c_uint) -> NvmlReturn {
    unsafe { uint(device, output) }
}

fn fixture(available: &AtomicBool) -> Nvml {
    Nvml {
        #[cfg(target_os = "linux")]
        process_functions: Default::default(),
        #[cfg(target_os = "linux")]
        process_states: vec![Default::default()],
        #[cfg(unix)]
        _library: DynamicLibrary {
            handle: Default::default(),
        },
        #[cfg(windows)]
        _library: DynamicLibrary(0),
        functions: NvmlFunctions {
            shutdown,
            get_name: name,
            get_power_limit: uint,
            get_temperature_threshold: kind_uint,
            get_utilization: utilization,
            get_clock: kind_uint,
            get_power: uint,
            get_power_state: uint,
            get_temperature: kind_uint,
            get_memory: memory,
            get_pcie: kind_uint,
            get_encoder: codec,
            get_decoder: codec,
        },
        devices: vec![(available as *const AtomicBool).cast_mut().cast()],
        names: vec!["test GPU".into()],
        #[cfg(windows)]
        driver_label: None,
        power_limits: vec![100],
        temperature_limits: vec![90],
    }
}

#[test]
fn nvml_metrics_follow_current_read_success_and_recover() {
    for initial in [true, false] {
        let available = AtomicBool::new(initial);
        let nvml = fixture(&available);
        for succeeds in [initial, !initial, initial] {
            available.store(succeeds, Ordering::Relaxed);
            let sample = nvml.collect(true, true).remove(0);
            for valid in [
                sample.support.utilization,
                sample.support.memory_utilization,
                sample.support.power,
                sample.support.power_state,
                sample.support.gpu_clock,
                sample.support.memory_clock,
                sample.support.temperature,
                sample.support.memory_total,
                sample.support.memory_used,
                sample.support.pcie,
                sample.support.encoder,
                sample.support.decoder,
            ] {
                assert_eq!(valid, succeeds, "initial={initial}, current={succeeds}");
            }
            if succeeds {
                assert_eq!(sample.utilization, 42);
                assert_eq!(sample.memory_used, 40);
            }
        }
    }
}

#[test]
fn nvml_disabled_metrics_are_not_marked_as_valid_zeroes() {
    let available = AtomicBool::new(true);
    let nvml = fixture(&available);
    let sample = nvml.collect(false, false).remove(0);
    assert!(!sample.support.temperature);
    assert!(!sample.support.pcie);
    assert!(sample.support.utilization);
}
