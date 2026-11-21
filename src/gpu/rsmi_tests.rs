use super::*;
use std::cell::Cell;

thread_local! {
    static AVAILABLE: Cell<bool> = const { Cell::new(true) };
}

unsafe extern "C" fn shutdown() -> RsmiStatus {
    RSMI_SUCCESS
}
unsafe extern "C" fn name(_: u32, _: *mut c_char, _: usize) -> RsmiStatus {
    1
}
unsafe extern "C" fn uint(_: u32, output: *mut u32) -> RsmiStatus {
    if AVAILABLE.get() {
        unsafe { *output = 42 };
        RSMI_SUCCESS
    } else {
        1
    }
}
unsafe extern "C" fn power(_: u32, _: u32, output: *mut u64) -> RsmiStatus {
    if AVAILABLE.get() {
        unsafe { *output = 42_000 };
        RSMI_SUCCESS
    } else {
        1
    }
}
unsafe extern "C" fn temperature(_: u32, _: u32, _: c_int, output: *mut i64) -> RsmiStatus {
    if AVAILABLE.get() {
        unsafe { *output = 42_000 };
        RSMI_SUCCESS
    } else {
        1
    }
}
unsafe extern "C" fn memory(index: u32, _: c_int, output: *mut u64) -> RsmiStatus {
    unsafe { power(index, 0, output) }
}
unsafe extern "C" fn pcie(_: u32, tx: *mut u64, rx: *mut u64, packet: *mut u64) -> RsmiStatus {
    if AVAILABLE.get() {
        unsafe {
            *tx = 1;
            *rx = 2;
            *packet = 1024;
        }
        RSMI_SUCCESS
    } else {
        1
    }
}
unsafe extern "C" fn clock(_: u32, _: c_int, output: *mut RsmiFrequenciesV6) -> RsmiStatus {
    if AVAILABLE.get() {
        unsafe {
            (*output).count = 1;
            (*output).frequencies[0] = 42_000_000;
        }
        RSMI_SUCCESS
    } else {
        1
    }
}

#[test]
fn rsmi_metrics_follow_current_read_success_and_recover() {
    for initial in [true, false] {
        AVAILABLE.set(initial);
        let rsmi = Rsmi {
            _library: DynamicLibrary {
                handle: ptr::null_mut(),
            },
            functions: RsmiFunctions {
                shutdown,
                name,
                power_cap: power,
                temperature,
                busy: uint,
                memory_busy: uint,
                clock: RsmiClock::V6(clock),
                power,
                memory_total: memory,
                memory_used: memory,
                pcie,
            },
            count: 1,
            names: vec!["test GPU".into()],
            power_limits: vec![100],
            temperature_limits: vec![90],
        };
        for succeeds in [initial, !initial, initial] {
            AVAILABLE.set(succeeds);
            let sample = rsmi.collect(true, true).remove(0);
            for valid in [
                sample.support.utilization,
                sample.support.memory_utilization,
                sample.support.gpu_clock,
                sample.support.memory_clock,
                sample.support.power,
                sample.support.temperature,
                sample.support.memory_total,
                sample.support.memory_used,
                sample.support.pcie,
            ] {
                assert_eq!(valid, succeeds, "initial={initial}, current={succeeds}");
            }
            if succeeds {
                assert_eq!(sample.utilization, 42);
                assert_eq!(sample.gpu_clock_mhz, 42);
                assert_eq!(sample.power_mw, 42);
            }
        }
        let sample = rsmi.collect(false, false).remove(0);
        assert!(!sample.support.temperature);
        assert!(!sample.support.pcie);
    }
}
