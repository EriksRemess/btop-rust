use crate::config::Config;
use crate::sensors;

#[derive(Debug, Clone, Default)]
pub struct GpuSupport {
    pub utilization: bool,
    pub memory_utilization: bool,
    pub gpu_clock: bool,
    pub memory_clock: bool,
    pub power: bool,
    pub power_state: bool,
    pub temperature: bool,
    pub memory_total: bool,
    pub memory_used: bool,
    pub unified_memory: bool,
    pub pcie: bool,
    pub encoder: bool,
    pub decoder: bool,
    pub encoder_power: bool,
    pub decoder_power: bool,
    pub encoder_bandwidth: bool,
    pub decoder_bandwidth: bool,
    pub encoder_sessions: bool,
    pub decoder_sessions: bool,
}

#[derive(Debug, Clone)]
pub struct GpuSample {
    pub name: String,
    pub driver_label: Option<String>,
    pub utilization: u32,
    pub memory_utilization: u32,
    pub gpu_clock_mhz: u32,
    pub memory_clock_mhz: u32,
    pub power_mw: u64,
    pub power_limit_mw: u64,
    pub power_state: i32,
    pub temperature_c: i64,
    pub temperature_max_c: i64,
    pub memory_total: u64,
    pub memory_used: u64,
    pub pcie_tx_kib: i64,
    pub pcie_rx_kib: i64,
    pub encoder_utilization: u32,
    pub decoder_utilization: u32,
    pub encoder_power_mw: f64,
    pub decoder_power_mw: f64,
    pub encoder_read_bps: u64,
    pub encoder_write_bps: u64,
    pub decoder_read_bps: u64,
    pub decoder_write_bps: u64,
    pub encoder_sessions: u32,
    pub decoder_sessions: u32,
    pub support: GpuSupport,
}

impl Default for GpuSample {
    fn default() -> Self {
        Self {
            name: String::new(),
            driver_label: None,
            utilization: 0,
            memory_utilization: 0,
            gpu_clock_mhz: 0,
            memory_clock_mhz: 0,
            power_mw: 0,
            power_limit_mw: 255_000,
            power_state: 32,
            temperature_c: 0,
            temperature_max_c: 110,
            memory_total: 0,
            memory_used: 0,
            pcie_tx_kib: -1,
            pcie_rx_kib: -1,
            encoder_utilization: 0,
            decoder_utilization: 0,
            encoder_power_mw: 0.0,
            decoder_power_mw: 0.0,
            encoder_read_bps: 0,
            encoder_write_bps: 0,
            decoder_read_bps: 0,
            decoder_write_bps: 0,
            encoder_sessions: 0,
            decoder_sessions: 0,
            support: GpuSupport::default(),
        }
    }
}

use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::ptr;
type CfRef = *const c_void;
struct OwnedCf(CfRef);
impl Drop for OwnedCf {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}
fn cf_string(s: &str) -> Option<OwnedCf> {
    let s = CString::new(s).ok()?;
    let p = unsafe { CFStringCreateWithCString(ptr::null(), s.as_ptr(), 0x08000100) };
    (!p.is_null()).then_some(OwnedCf(p))
}
fn property(service: u32, key: &str) -> Option<OwnedCf> {
    let key = cf_string(key)?;
    let value = unsafe {
        IORegistryEntrySearchCFProperty(service, c"IOService".as_ptr(), key.0, ptr::null(), 3)
    };
    (!value.is_null()).then_some(OwnedCf(value))
}
fn number(value: CfRef) -> Option<u64> {
    if value.is_null() || unsafe { CFGetTypeID(value) != CFNumberGetTypeID() } {
        return None;
    }
    let mut n = 0_i64;
    (unsafe { CFNumberGetValue(value, 4, (&mut n as *mut i64).cast()) } && n >= 0)
        .then_some(n as u64)
}
fn text(value: CfRef) -> Option<String> {
    if unsafe { CFGetTypeID(value) == CFDataGetTypeID() } {
        let n = unsafe { CFDataGetLength(value) };
        if !(1..=4096).contains(&n) {
            return None;
        }
        let p = unsafe { CFDataGetBytePtr(value) };
        if p.is_null() {
            return None;
        }
        let bytes = unsafe { std::slice::from_raw_parts(p, n as usize) };
        Some(
            String::from_utf8_lossy(bytes.split(|b| *b == 0).next()?)
                .trim()
                .into(),
        )
    } else if unsafe { CFGetTypeID(value) == CFStringGetTypeID() } {
        let mut buffer = [0_i8; 512];
        if unsafe {
            CFStringGetCString(
                value,
                buffer.as_mut_ptr(),
                buffer.len() as isize,
                0x08000100,
            )
        } {
            Some(
                unsafe { CStr::from_ptr(buffer.as_ptr()) }
                    .to_string_lossy()
                    .into_owned(),
            )
        } else {
            None
        }
    } else {
        None
    }
}
fn dict_number(dict: CfRef, key: &str) -> Option<u64> {
    let key = cf_string(key)?;
    number(unsafe { CFDictionaryGetValue(dict, key.0) })
}
fn vram(total: Option<u64>, free: Option<u64>) -> Option<(u64, Option<u64>)> {
    let total = total.filter(|n| *n > 0)?;
    Some((total, free.filter(|f| *f <= total).map(|f| total - f)))
}

pub struct GpuCollector {
    service: u32,
    name: String,
}
impl GpuCollector {
    pub fn new(_config: &Config) -> Self {
        let mut iterator = 0;
        let matching = unsafe { IOServiceMatching(c"IOAccelerator".as_ptr()) };
        let service = if !matching.is_null()
            && unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } == 0
        {
            let s = unsafe { IOIteratorNext(iterator) };
            unsafe { IOObjectRelease(iterator) };
            s
        } else {
            0
        };
        let name = property(service, "model")
            .and_then(|v| text(v.0))
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "Radeon GPU".into());
        Self { service, name }
    }
    pub fn collect(&mut self, config: &Config) -> Vec<GpuSample> {
        if self.service == 0 || !config.value("shown_gpus").unwrap_or("amd").contains("amd") {
            return Vec::new();
        }
        let mut sample = GpuSample {
            name: self.name.clone(),
            driver_label: Some("Snow Leopard IOKit".into()),
            power_limit_mw: 0,
            ..GpuSample::default()
        };
        if let Some(stats) = property(self.service, "PerformanceStatistics")
            && unsafe { CFGetTypeID(stats.0) == CFDictionaryGetTypeID() }
            && let Some((total, used)) = vram(
                dict_number(stats.0, "vramTotalBytes")
                    .or_else(|| property(self.service, "VRAM,totalsize").and_then(|v| number(v.0))),
                dict_number(stats.0, "vramFreeBytes"),
            )
        {
            sample.memory_total = total;
            sample.support.memory_total = true;
            if let Some(used) = used {
                sample.memory_used = used;
                sample.support.memory_used = true;
            }
        }
        if config.check_temperature
            && let Some(t) = sensors::temperature(*b"TG0D")
        {
            sample.temperature_c = t.round() as i64;
            sample.temperature_max_c = 100;
            sample.support.temperature = true;
        }
        vec![sample]
    }
}
impl Drop for GpuCollector {
    fn drop(&mut self) {
        if self.service != 0 {
            unsafe { IOObjectRelease(self.service) };
        }
    }
}
pub fn diagnostics() -> String {
    let config = Config::default();
    let mut collector = GpuCollector::new(&config);
    let mut out = String::from("Legacy Radeon GPU\n");
    for gpu in collector.collect(&config) {
        out.push_str(&format!("  model: {}\n", gpu.name));
        if gpu.support.temperature {
            out.push_str(&format!("  temperature: {} C\n", gpu.temperature_c));
        }
        if gpu.support.memory_total {
            out.push_str(&format!("  VRAM total: {} bytes\n", gpu.memory_total));
        }
        if gpu.support.memory_used {
            out.push_str(&format!("  VRAM used: {} bytes\n", gpu.memory_used));
        }
    }
    out.push_str("  GPU utilization, clocks, power, PCIe and media engines: unavailable\n");
    out
}
#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CfRef;
    fn IOServiceGetMatchingServices(port: u32, matching: CfRef, iterator: *mut u32) -> c_int;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IORegistryEntrySearchCFProperty(
        entry: u32,
        plane: *const c_char,
        key: CfRef,
        allocator: CfRef,
        options: u32,
    ) -> CfRef;
}
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: CfRef);
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFStringCreateWithCString(allocator: CfRef, value: *const c_char, encoding: u32) -> CfRef;
    fn CFStringGetCString(value: CfRef, buffer: *mut c_char, size: isize, encoding: u32) -> bool;
    fn CFStringGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFDataGetBytePtr(value: CfRef) -> *const u8;
    fn CFDataGetLength(value: CfRef) -> isize;
    fn CFNumberGetTypeID() -> usize;
    fn CFNumberGetValue(value: CfRef, kind: c_int, result: *mut c_void) -> bool;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFDictionaryGetValue(value: CfRef, key: CfRef) -> CfRef;
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_inconsistent_vram_counters() {
        assert_eq!(vram(Some(128), Some(32)), Some((128, Some(96))));
        assert_eq!(vram(Some(128), Some(129)), Some((128, None)));
        assert_eq!(vram(Some(0), Some(0)), None);
    }
}
