use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem;
use std::ptr;
use std::time::Instant;

use super::{GpuSample, GpuSupport};

type CfRef = *const c_void;
type IoReportSubscription = *mut c_void;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT32: c_int = 3;
const HOST_VM_INFO64: c_int = 4;
const HID_PAGE_APPLE_VENDOR: i32 = 0xff00;
const HID_USAGE_TEMPERATURE: i32 = 5;
const HID_EVENT_TEMPERATURE: i64 = 15;
const KERNEL_INDEX_SMC: u32 = 2;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_READ_KEYINFO: u8 = 9;
const A18_GPU_TEMPERATURE_KEYS: [[u8; 4]; 8] = [
    *b"Tg04", *b"Tg05", *b"Tg0C", *b"Tg0D", *b"Tg0K", *b"Tg0L", *b"Tg0d", *b"Tg0e",
];

pub(crate) struct AppleGpuCollector {
    subscription: IoReportSubscription,
    channels: CfRef,
    previous: CfRef,
    previous_at: Instant,
    name: String,
    frequencies_mhz: Vec<u32>,
    memory_total: u64,
    power_limit_mw: u64,
    smc: Option<AppleSmc>,
}

impl AppleGpuCollector {
    pub(crate) fn new() -> Option<Self> {
        let gpu_group = cf_string("GPU Stats")?;
        let gpu_subgroup = cf_string("GPU Performance States")?;
        let energy_group = cf_string("Energy Model")?;
        let gpu_channels = unsafe { IOReportCopyChannelsInGroup(gpu_group, gpu_subgroup, 0, 0, 0) };
        let energy_channels =
            unsafe { IOReportCopyChannelsInGroup(energy_group, ptr::null(), 0, 0, 0) };
        unsafe {
            CFRelease(gpu_group);
            CFRelease(gpu_subgroup);
            CFRelease(energy_group);
        }
        if gpu_channels.is_null() && energy_channels.is_null() {
            return None;
        }
        if !gpu_channels.is_null() && !energy_channels.is_null() {
            unsafe { IOReportMergeChannels(gpu_channels, energy_channels, ptr::null()) };
        }
        let base = if !gpu_channels.is_null() {
            gpu_channels
        } else {
            energy_channels
        };
        let channels =
            unsafe { CFDictionaryCreateMutableCopy(ptr::null(), CFDictionaryGetCount(base), base) };
        unsafe {
            if !gpu_channels.is_null() {
                CFRelease(gpu_channels);
            }
            if !energy_channels.is_null() {
                CFRelease(energy_channels);
            }
        }
        if channels.is_null() {
            return None;
        }
        let mut subscribed_channels = ptr::null();
        let subscription = unsafe {
            IOReportCreateSubscription(
                ptr::null_mut(),
                channels,
                &mut subscribed_channels,
                0,
                ptr::null(),
            )
        };
        if !subscribed_channels.is_null() {
            unsafe { CFRelease(subscribed_channels) };
        }
        if subscription.is_null() {
            unsafe { CFRelease(channels) };
            return None;
        }
        let previous = unsafe { IOReportCreateSamples(subscription, channels, ptr::null()) };
        let chip = sysctl_string("machdep.cpu.brand_string")
            .unwrap_or_else(|| "Apple Silicon".to_string());
        Some(Self {
            subscription,
            channels,
            previous,
            previous_at: Instant::now(),
            name: format!("{} GPU", chip.trim()),
            frequencies_mhz: gpu_frequencies(),
            memory_total: sysctl_u64("hw.memsize").unwrap_or(0),
            power_limit_mw: 20_000,
            smc: AppleSmc::new(),
        })
    }

    pub(crate) fn collect(&mut self, check_temperature: bool) -> GpuSample {
        let elapsed = self.previous_at.elapsed().as_secs_f64().max(0.001);
        self.previous_at = Instant::now();
        let current =
            unsafe { IOReportCreateSamples(self.subscription, self.channels, ptr::null()) };
        let delta = if current.is_null() || self.previous.is_null() {
            ptr::null()
        } else {
            unsafe { IOReportCreateSamplesDelta(self.previous, current, ptr::null()) }
        };
        if !self.previous.is_null() {
            unsafe { CFRelease(self.previous) };
        }
        self.previous = current;

        let mut utilization = 0_u32;
        let mut gpu_clock_mhz = 0_u32;
        let mut power_mw = 0_u64;
        if !delta.is_null() {
            self.parse_delta(
                delta,
                elapsed,
                &mut utilization,
                &mut gpu_clock_mhz,
                &mut power_mw,
            );
            unsafe { CFRelease(delta) };
        }
        self.power_limit_mw = self.power_limit_mw.max(power_mw);
        let memory_used = gpu_memory_used().unwrap_or(0).min(self.memory_total);
        let temperature = check_temperature
            .then(|| {
                read_gpu_temperature().or_else(|| {
                    self.smc
                        .as_ref()
                        .and_then(AppleSmc::read_a18_gpu_temperature)
                })
            })
            .flatten()
            .unwrap_or(0.0)
            .round() as i64;

        GpuSample {
            name: self.name.clone(),
            utilization,
            memory_utilization: utilization,
            gpu_clock_mhz,
            power_mw,
            power_limit_mw: self.power_limit_mw,
            temperature_c: temperature,
            temperature_max_c: 110,
            memory_total: self.memory_total,
            memory_used,
            support: GpuSupport {
                utilization: true,
                memory_utilization: true,
                gpu_clock: !self.frequencies_mhz.is_empty(),
                power: true,
                temperature: true,
                memory_total: self.memory_total > 0,
                memory_used: true,
                ..GpuSupport::default()
            },
            ..GpuSample::default()
        }
    }

    fn parse_delta(
        &self,
        delta: CfRef,
        elapsed: f64,
        utilization: &mut u32,
        gpu_clock_mhz: &mut u32,
        power_mw: &mut u64,
    ) {
        let Some(key) = cf_string("IOReportChannels") else {
            return;
        };
        let channel_array = unsafe { CFDictionaryGetValue(delta, key) };
        unsafe { CFRelease(key) };
        if channel_array.is_null() {
            return;
        }
        let count = unsafe { CFArrayGetCount(channel_array) };
        for index in 0..count {
            let item = unsafe { CFArrayGetValueAtIndex(channel_array, index) };
            if item.is_null() {
                continue;
            }
            let group = cf_string_value(unsafe { IOReportChannelGetGroup(item) });
            let subgroup = cf_string_value(unsafe { IOReportChannelGetSubGroup(item) });
            let channel = cf_string_value(unsafe { IOReportChannelGetChannelName(item) });
            if group == "GPU Stats" && subgroup == "GPU Performance States" && channel == "GPUPH" {
                let states = unsafe { IOReportStateGetCount(item) };
                let mut total = 0_i64;
                let mut active_start = 0;
                for state in 0..states {
                    let name =
                        cf_string_value(unsafe { IOReportStateGetNameForIndex(item, state) });
                    if matches!(name.as_str(), "IDLE" | "OFF" | "DOWN") {
                        active_start = state + 1;
                    }
                    let residency = unsafe { IOReportStateGetResidency(item, state) }.max(0);
                    total = total.saturating_add(residency);
                }
                let active = (active_start..states).fold(0_i64, |sum, state| {
                    sum.saturating_add(unsafe { IOReportStateGetResidency(item, state) }.max(0))
                });
                if total > 0 {
                    *utilization = ((active as f64 * 100.0 / total as f64).round() as u32).min(100);
                }
                if active > 0 && !self.frequencies_mhz.is_empty() {
                    let weighted = (active_start..states)
                        .zip(self.frequencies_mhz.iter().copied())
                        .fold(0_f64, |sum, (state, frequency)| {
                            let residency =
                                unsafe { IOReportStateGetResidency(item, state) }.max(0) as f64;
                            sum + residency * f64::from(frequency)
                        });
                    *gpu_clock_mhz = (weighted / active as f64).round() as u32;
                }
            } else if group == "Energy Model" && channel == "GPU Energy" {
                let unit = cf_string_value(unsafe { IOReportChannelGetUnitLabel(item) });
                let value = unsafe { IOReportSimpleGetIntegerValue(item, 0) }.max(0) as f64;
                let joules = if unit.contains("nJ") {
                    value / 1_000_000_000.0
                } else if unit.contains("uJ") || unit.contains("µJ") {
                    value / 1_000_000.0
                } else if unit.contains("mJ") {
                    value / 1_000.0
                } else {
                    value
                };
                *power_mw = (joules * 1_000.0 / elapsed).round().max(0.0) as u64;
            }
        }
    }
}

impl Drop for AppleGpuCollector {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                CFRelease(self.previous);
            }
            if !self.channels.is_null() {
                CFRelease(self.channels);
            }
            if !self.subscription.is_null() {
                CFRelease(self.subscription);
            }
        }
    }
}

pub(crate) fn read_cpu_temperatures(core_count: usize) -> (Option<f64>, Vec<Option<f64>>) {
    let sensors = thermal_sensors();
    let mut accelerators = Vec::new();
    let mut dies = Vec::new();
    let mut soc = Vec::new();
    let mut named_accelerators: BTreeMap<String, Vec<f64>> = BTreeMap::new();
    let mut indexed_dies: BTreeMap<usize, Vec<f64>> = BTreeMap::new();
    for (name, value) in sensors {
        if name.starts_with("eACC") || name.starts_with("pACC") {
            accelerators.push(value);
            named_accelerators.entry(name).or_default().push(value);
        } else if let Some(index) = sensor_index(&name, "PMU tdie") {
            dies.push(value);
            indexed_dies.entry(index).or_default().push(value);
        } else if name.starts_with("SOC MTR Temp Sensor") {
            soc.push(value);
        }
    }
    let package_values = if !accelerators.is_empty() {
        &accelerators
    } else if !dies.is_empty() {
        &dies
    } else {
        &soc
    };
    let package = average(package_values);
    let values: Vec<f64> = if !indexed_dies.is_empty() {
        indexed_dies
            .values()
            .filter_map(|values| average(values))
            .collect()
    } else {
        named_accelerators
            .values()
            .filter_map(|values| average(values))
            .collect()
    };
    let mut cores = vec![None; core_count];
    for (slot, value) in cores.iter_mut().zip(values) {
        *slot = Some(value);
    }
    (package, cores)
}

struct AppleSmc {
    connection: u32,
}

impl AppleSmc {
    fn new() -> Option<Self> {
        let matching = unsafe { IOServiceMatching(c"AppleSMC".as_ptr()) };
        if matching.is_null() {
            return None;
        }
        let mut iterator = 0_u32;
        if unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0 {
            return None;
        }
        let service = unsafe { IOIteratorNext(iterator) };
        unsafe { IOObjectRelease(iterator) };
        if service == 0 {
            return None;
        }
        let mut connection = 0_u32;
        let result = unsafe { IOServiceOpen(service, mach_task_self_, 0, &mut connection) };
        unsafe { IOObjectRelease(service) };
        (result == 0 && connection != 0).then_some(Self { connection })
    }

    fn read_a18_gpu_temperature(&self) -> Option<f64> {
        let values = A18_GPU_TEMPERATURE_KEYS
            .iter()
            .filter_map(|key| self.read_temperature(*key))
            .collect::<Vec<_>>();
        average(&values)
    }

    fn read_temperature(&self, key: [u8; 4]) -> Option<f64> {
        let mut input = SmcKeyData {
            key: u32::from_be_bytes(key),
            data8: SMC_CMD_READ_KEYINFO,
            ..SmcKeyData::default()
        };
        let mut output = SmcKeyData::default();
        let mut output_size = mem::size_of::<SmcKeyData>();
        if unsafe {
            IOConnectCallStructMethod(
                self.connection,
                KERNEL_INDEX_SMC,
                (&input as *const SmcKeyData).cast(),
                mem::size_of::<SmcKeyData>(),
                (&mut output as *mut SmcKeyData).cast(),
                &mut output_size,
            )
        } != 0
        {
            return None;
        }
        let data_size = output.key_info.data_size;
        let data_type = output.key_info.data_type;
        input.key_info.data_size = data_size;
        input.data8 = SMC_CMD_READ_BYTES;
        output = SmcKeyData::default();
        output_size = mem::size_of::<SmcKeyData>();
        if unsafe {
            IOConnectCallStructMethod(
                self.connection,
                KERNEL_INDEX_SMC,
                (&input as *const SmcKeyData).cast(),
                mem::size_of::<SmcKeyData>(),
                (&mut output as *mut SmcKeyData).cast(),
                &mut output_size,
            )
        } != 0
        {
            return None;
        }
        decode_smc_temperature(data_size, data_type, &output.bytes)
    }
}

impl Drop for AppleSmc {
    fn drop(&mut self) {
        unsafe { IOServiceClose(self.connection) };
    }
}

fn decode_smc_temperature(data_size: u32, data_type: u32, bytes: &[u8; 32]) -> Option<f64> {
    let value = match (data_type.to_be_bytes(), data_size) {
        (kind, size) if kind == *b"flt " && size >= 4 => {
            f64::from(f32::from_le_bytes(bytes[..4].try_into().ok()?))
        }
        (kind, size) if kind == *b"sp78" && size >= 2 => {
            f64::from(i16::from_be_bytes(bytes[..2].try_into().ok()?)) / 256.0
        }
        _ => return None,
    };
    (value.is_finite() && value > 0.0 && value < 150.0).then_some(value)
}

fn read_gpu_temperature() -> Option<f64> {
    let values = thermal_sensors()
        .into_iter()
        .filter_map(|(name, value)| {
            (name.contains("GPU") || (name.starts_with("PMU TP") && name.ends_with('g')))
                .then_some(value)
        })
        .collect::<Vec<_>>();
    average(&values)
}

fn thermal_sensors() -> Vec<(String, f64)> {
    unsafe {
        let matching = thermal_matching_dictionary();
        if matching.is_null() {
            return Vec::new();
        }
        let client = IOHIDEventSystemClientCreate(ptr::null());
        if client.is_null() {
            CFRelease(matching);
            return Vec::new();
        }
        IOHIDEventSystemClientSetMatching(client, matching);
        let services = IOHIDEventSystemClientCopyServices(client);
        let mut result = Vec::new();
        if !services.is_null() {
            let product_key = cf_string("Product");
            for index in 0..CFArrayGetCount(services) {
                let service = CFArrayGetValueAtIndex(services, index);
                if service.is_null() || product_key.is_none() {
                    continue;
                }
                let property = IOHIDServiceClientCopyProperty(service, product_key.unwrap());
                let event = IOHIDServiceClientCopyEvent(service, HID_EVENT_TEMPERATURE, 0, 0);
                if !property.is_null() && !event.is_null() {
                    let name = cf_string_value(property);
                    let value =
                        IOHIDEventGetFloatValue(event, (HID_EVENT_TEMPERATURE << 16) as i32);
                    if !name.is_empty() && value > 0.0 && value < 150.0 {
                        result.push((name, value));
                    }
                }
                if !property.is_null() {
                    CFRelease(property);
                }
                if !event.is_null() {
                    CFRelease(event);
                }
            }
            if let Some(key) = product_key {
                CFRelease(key);
            }
            CFRelease(services);
        }
        CFRelease(client);
        CFRelease(matching);
        result
    }
}

fn thermal_matching_dictionary() -> CfRef {
    let Some(page_key) = cf_string("PrimaryUsagePage") else {
        return ptr::null();
    };
    let Some(usage_key) = cf_string("PrimaryUsage") else {
        unsafe { CFRelease(page_key) };
        return ptr::null();
    };
    let page = unsafe {
        CFNumberCreate(
            ptr::null(),
            CF_NUMBER_SINT32,
            (&HID_PAGE_APPLE_VENDOR as *const i32).cast(),
        )
    };
    let usage = unsafe {
        CFNumberCreate(
            ptr::null(),
            CF_NUMBER_SINT32,
            (&HID_USAGE_TEMPERATURE as *const i32).cast(),
        )
    };
    let keys = [page_key, usage_key];
    let values = [page, usage];
    let dictionary = if page.is_null() || usage.is_null() {
        ptr::null()
    } else {
        unsafe {
            CFDictionaryCreate(
                ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                2,
                &kCFTypeDictionaryKeyCallBacks,
                &kCFTypeDictionaryValueCallBacks,
            )
        }
    };
    unsafe {
        CFRelease(page_key);
        CFRelease(usage_key);
    }
    if !page.is_null() {
        unsafe { CFRelease(page) };
    }
    if !usage.is_null() {
        unsafe { CFRelease(usage) };
    }
    dictionary
}

fn sensor_index(name: &str, prefix: &str) -> Option<usize> {
    name.strip_prefix(prefix)?
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>()
        .parse()
        .ok()
}

fn average(values: &[f64]) -> Option<f64> {
    (!values.is_empty()).then(|| values.iter().sum::<f64>() / values.len() as f64)
}

fn gpu_frequencies() -> Vec<u32> {
    let Ok(class) = CString::new("AppleARMIODevice") else {
        return Vec::new();
    };
    let mut iterator = 0_u32;
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null()
        || unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0
    {
        return Vec::new();
    }
    let mut frequencies = Vec::new();
    loop {
        let entry = unsafe { IOIteratorNext(iterator) };
        if entry == 0 {
            break;
        }
        let mut name = [0_i8; 128];
        let is_pmgr = unsafe { IORegistryEntryGetName(entry, name.as_mut_ptr()) } == 0
            && unsafe { CStr::from_ptr(name.as_ptr()) }.to_bytes() == b"pmgr";
        if is_pmgr {
            let mut properties = ptr::null();
            if unsafe { IORegistryEntryCreateCFProperties(entry, &mut properties, ptr::null(), 0) }
                == 0
                && !properties.is_null()
            {
                if let Some(key) = cf_string("voltage-states9") {
                    let data = unsafe { CFDictionaryGetValue(properties, key) };
                    if !data.is_null() {
                        let length = unsafe { CFDataGetLength(data) }.max(0) as usize;
                        let bytes = unsafe { CFDataGetBytePtr(data) };
                        if !bytes.is_null() {
                            let values = unsafe { std::slice::from_raw_parts(bytes, length) };
                            for pair in values.chunks_exact(8) {
                                let hz = u32::from_ne_bytes(pair[..4].try_into().unwrap());
                                if hz > 0 {
                                    frequencies.push(hz / 1_000_000);
                                }
                            }
                        }
                    }
                    unsafe { CFRelease(key) };
                }
                unsafe { CFRelease(properties) };
            }
        }
        unsafe { IOObjectRelease(entry) };
    }
    unsafe { IOObjectRelease(iterator) };
    frequencies
}

fn gpu_memory_used() -> Option<u64> {
    let page_size = sysctl_u64("hw.pagesize").unwrap_or(4096);
    let mut stats = VmStatistics64::default();
    let mut count = (mem::size_of::<VmStatistics64>() / mem::size_of::<c_int>()) as u32;
    if unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            (&mut stats as *mut VmStatistics64).cast(),
            &mut count,
        )
    } != 0
    {
        return None;
    }
    let pages = u64::from(stats.active_count)
        .saturating_add(u64::from(stats.inactive_count))
        .saturating_add(u64::from(stats.wire_count))
        .saturating_add(u64::from(stats.speculative_count))
        .saturating_add(u64::from(stats.compressor_page_count))
        .saturating_sub(u64::from(stats.purgeable_count))
        .saturating_sub(u64::from(stats.external_page_count));
    Some(pages.saturating_mul(page_size))
}

fn sysctl_u64(name: &str) -> Option<u64> {
    let name = CString::new(name).ok()?;
    let mut value = 0_u64;
    let mut size = mem::size_of::<u64>();
    (unsafe {
        sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut u64).cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } == 0)
        .then_some(value)
}

fn sysctl_string(name: &str) -> Option<String> {
    let name = CString::new(name).ok()?;
    let mut size = 0;
    if unsafe {
        sysctlbyname(
            name.as_ptr(),
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
        || size == 0
    {
        return None;
    }
    let mut buffer = vec![0_u8; size];
    if unsafe {
        sysctlbyname(
            name.as_ptr(),
            buffer.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return None;
    }
    Some(
        CStr::from_bytes_until_nul(&buffer)
            .ok()?
            .to_string_lossy()
            .into_owned(),
    )
}

fn cf_string(value: &str) -> Option<CfRef> {
    let value = CString::new(value).ok()?;
    let string =
        unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), CF_STRING_ENCODING_UTF8) };
    (!string.is_null()).then_some(string)
}

fn cf_string_value(value: CfRef) -> String {
    if value.is_null() {
        return String::new();
    }
    let mut buffer = [0_i8; 256];
    if unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            CF_STRING_ENCODING_UTF8,
        )
    } {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    } else {
        String::new()
    }
}

#[repr(C)]
struct CfDictionaryKeyCallbacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
    hash: *const c_void,
}

#[repr(C)]
struct CfDictionaryValueCallbacks {
    version: isize,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
    equal: *const c_void,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyDataVersion {
    major: u8,
    minor: u8,
    build: u8,
    reserved: u8,
    release: u16,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyDataLimit {
    version: u16,
    length: u16,
    cpu_limit: u32,
    gpu_limit: u32,
    memory_limit: u32,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyInfo {
    data_size: u32,
    data_type: u32,
    attributes: u8,
}

#[repr(C)]
#[derive(Default)]
struct SmcKeyData {
    key: u32,
    version: SmcKeyDataVersion,
    limit: SmcKeyDataLimit,
    key_info: SmcKeyInfo,
    result: u8,
    status: u8,
    data8: u8,
    data32: u32,
    bytes: [u8; 32],
}

#[repr(C)]
#[derive(Default)]
struct VmStatistics64 {
    free_count: u32,
    active_count: u32,
    inactive_count: u32,
    wire_count: u32,
    zero_fill_count: u64,
    reactivations: u64,
    pageins: u64,
    pageouts: u64,
    faults: u64,
    cow_faults: u64,
    lookups: u64,
    hits: u64,
    purges: u64,
    purgeable_count: u32,
    speculative_count: u32,
    decompressions: u64,
    compressions: u64,
    swapins: u64,
    swapouts: u64,
    compressor_page_count: u32,
    throttled_count: u32,
    external_page_count: u32,
    internal_page_count: u32,
    total_uncompressed_pages_in_compressor: u64,
}

#[link(name = "IOReport")]
unsafe extern "C" {
    fn IOReportCopyChannelsInGroup(group: CfRef, subgroup: CfRef, a: u64, b: u64, c: u64) -> CfRef;
    fn IOReportMergeChannels(a: CfRef, b: CfRef, null: CfRef);
    fn IOReportCreateSubscription(
        a: *mut c_void,
        channels: CfRef,
        subscribed_channels: *mut CfRef,
        d: u64,
        null: CfRef,
    ) -> IoReportSubscription;
    fn IOReportCreateSamples(
        subscription: IoReportSubscription,
        channels: CfRef,
        null: CfRef,
    ) -> CfRef;
    fn IOReportCreateSamplesDelta(previous: CfRef, current: CfRef, null: CfRef) -> CfRef;
    fn IOReportChannelGetGroup(item: CfRef) -> CfRef;
    fn IOReportChannelGetSubGroup(item: CfRef) -> CfRef;
    fn IOReportChannelGetChannelName(item: CfRef) -> CfRef;
    fn IOReportSimpleGetIntegerValue(item: CfRef, index: i32) -> i64;
    fn IOReportChannelGetUnitLabel(item: CfRef) -> CfRef;
    fn IOReportStateGetCount(item: CfRef) -> i32;
    fn IOReportStateGetNameForIndex(item: CfRef, index: i32) -> CfRef;
    fn IOReportStateGetResidency(item: CfRef, index: i32) -> i64;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CfRef;
    fn IOServiceGetMatchingServices(main_port: u32, matching: CfRef, iterator: *mut u32) -> c_int;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IOServiceOpen(service: u32, owning_task: u32, kind: u32, connection: *mut u32) -> c_int;
    fn IOServiceClose(connection: u32) -> c_int;
    fn IOConnectCallStructMethod(
        connection: u32,
        selector: u32,
        input: *const c_void,
        input_size: usize,
        output: *mut c_void,
        output_size: *mut usize,
    ) -> c_int;
    fn IORegistryEntryGetName(entry: u32, name: *mut c_char) -> c_int;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut CfRef,
        allocator: CfRef,
        options: u32,
    ) -> c_int;
    fn IOHIDEventSystemClientCreate(allocator: CfRef) -> CfRef;
    fn IOHIDEventSystemClientSetMatching(client: CfRef, matching: CfRef) -> c_int;
    fn IOHIDEventSystemClientCopyServices(client: CfRef) -> CfRef;
    fn IOHIDServiceClientCopyEvent(service: CfRef, kind: i64, a: i32, b: i64) -> CfRef;
    fn IOHIDServiceClientCopyProperty(service: CfRef, property: CfRef) -> CfRef;
    fn IOHIDEventGetFloatValue(event: CfRef, field: i32) -> f64;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: CfDictionaryKeyCallbacks;
    static kCFTypeDictionaryValueCallBacks: CfDictionaryValueCallbacks;
    fn CFRelease(value: CfRef);
    fn CFStringCreateWithCString(allocator: CfRef, value: *const c_char, encoding: u32) -> CfRef;
    fn CFStringGetCString(
        string: CfRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFNumberCreate(allocator: CfRef, kind: c_int, value: *const c_void) -> CfRef;
    fn CFDictionaryCreate(
        allocator: CfRef,
        keys: *const CfRef,
        values: *const CfRef,
        count: isize,
        key_callbacks: *const CfDictionaryKeyCallbacks,
        value_callbacks: *const CfDictionaryValueCallbacks,
    ) -> CfRef;
    fn CFDictionaryCreateMutableCopy(allocator: CfRef, capacity: isize, source: CfRef) -> CfRef;
    fn CFDictionaryGetCount(dictionary: CfRef) -> isize;
    fn CFDictionaryGetValue(dictionary: CfRef, key: CfRef) -> CfRef;
    fn CFDataGetLength(data: CfRef) -> isize;
    fn CFDataGetBytePtr(data: CfRef) -> *const u8;
    fn CFArrayGetCount(array: CfRef) -> isize;
    fn CFArrayGetValueAtIndex(array: CfRef, index: isize) -> CfRef;
}

unsafe extern "C" {
    static mach_task_self_: u32;
    fn mach_host_self() -> u32;
    fn host_statistics64(host: u32, flavor: c_int, info: *mut c_int, count: *mut u32) -> c_int;
    fn sysctlbyname(
        name: *const c_char,
        old: *mut c_void,
        old_size: *mut usize,
        new: *mut c_void,
        new_size: usize,
    ) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn apple_smc_layout_and_a18_float_temperature_match_the_native_abi() {
        assert_eq!(mem::size_of::<SmcKeyData>(), 80);
        let mut bytes = [0_u8; 32];
        bytes[..4].copy_from_slice(&[0x7e, 0xc1, 0x7a, 0x42]);
        let value = decode_smc_temperature(4, u32::from_be_bytes(*b"flt "), &bytes).unwrap();
        assert!((value - 62.688_957).abs() < 0.000_1);
    }
}
