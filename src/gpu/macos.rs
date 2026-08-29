use std::collections::BTreeMap;
use std::ffi::{CStr, CString, c_char, c_int, c_void};
use std::mem;
use std::ptr;
use std::time::Instant;

use super::{GpuSample, GpuSupport};

type CfRef = *const c_void;
type IoReportSubscription = *mut c_void;
type CopyChannelsInGroup = unsafe extern "C" fn(CfRef, CfRef, u64, u64, u64) -> CfRef;
type MergeChannels = unsafe extern "C" fn(CfRef, CfRef, CfRef);
type CreateSubscription =
    unsafe extern "C" fn(*mut c_void, CfRef, *mut CfRef, u64, CfRef) -> IoReportSubscription;
type CreateSamples = unsafe extern "C" fn(IoReportSubscription, CfRef, CfRef) -> CfRef;
type CreateSamplesDelta = unsafe extern "C" fn(CfRef, CfRef, CfRef) -> CfRef;
type ChannelString = unsafe extern "C" fn(CfRef) -> CfRef;
type SimpleInteger = unsafe extern "C" fn(CfRef, i32) -> i64;
type StateCount = unsafe extern "C" fn(CfRef) -> i32;
type StateName = unsafe extern "C" fn(CfRef, i32) -> CfRef;
type StateResidency = unsafe extern "C" fn(CfRef, i32) -> i64;
type HidClientCreate = unsafe extern "C" fn(CfRef) -> CfRef;
type HidSetMatching = unsafe extern "C" fn(CfRef, CfRef) -> c_int;
type HidCopyServices = unsafe extern "C" fn(CfRef) -> CfRef;
type HidCopyEvent = unsafe extern "C" fn(CfRef, i64, i32, i64) -> CfRef;
type HidCopyProperty = unsafe extern "C" fn(CfRef, CfRef) -> CfRef;
type HidEventFloat = unsafe extern "C" fn(CfRef, i32) -> f64;

const CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const CF_NUMBER_SINT32: c_int = 3;
const CF_NUMBER_SINT64: c_int = 4;
const HID_PAGE_APPLE_VENDOR: i32 = 0xff00;
const HID_USAGE_TEMPERATURE: i32 = 5;
const HID_EVENT_TEMPERATURE: i64 = 15;
const KERNEL_INDEX_SMC: u32 = 2;
const SMC_CMD_READ_BYTES: u8 = 5;
const SMC_CMD_READ_KEYINFO: u8 = 9;
const RTLD_LAZY: c_int = 0x1;
const RTLD_LOCAL: c_int = 0x4;
const A18_GPU_TEMPERATURE_KEYS: [[u8; 4]; 8] = [
    *b"Tg04", *b"Tg05", *b"Tg0C", *b"Tg0D", *b"Tg0K", *b"Tg0L", *b"Tg0d", *b"Tg0e",
];

struct IoReportApi {
    handle: *mut c_void,
    copy_channels: CopyChannelsInGroup,
    merge_channels: MergeChannels,
    create_subscription: CreateSubscription,
    create_samples: CreateSamples,
    create_samples_delta: CreateSamplesDelta,
    channel_group: ChannelString,
    channel_subgroup: ChannelString,
    channel_name: ChannelString,
    simple_integer: SimpleInteger,
    channel_unit: ChannelString,
    state_count: StateCount,
    state_name: StateName,
    state_residency: StateResidency,
}

impl IoReportApi {
    fn load() -> Option<Self> {
        let handle = unsafe {
            dlopen(
                c"/usr/lib/libIOReport.dylib".as_ptr(),
                RTLD_LAZY | RTLD_LOCAL,
            )
        };
        if handle.is_null() {
            return None;
        }
        macro_rules! symbol {
            ($name:expr) => {
                match unsafe { dynamic_symbol(handle, $name) } {
                    Some(symbol) => symbol,
                    None => {
                        unsafe { dlclose(handle) };
                        return None;
                    }
                }
            };
        }
        Some(Self {
            handle,
            copy_channels: symbol!(c"IOReportCopyChannelsInGroup"),
            merge_channels: symbol!(c"IOReportMergeChannels"),
            create_subscription: symbol!(c"IOReportCreateSubscription"),
            create_samples: symbol!(c"IOReportCreateSamples"),
            create_samples_delta: symbol!(c"IOReportCreateSamplesDelta"),
            channel_group: symbol!(c"IOReportChannelGetGroup"),
            channel_subgroup: symbol!(c"IOReportChannelGetSubGroup"),
            channel_name: symbol!(c"IOReportChannelGetChannelName"),
            simple_integer: symbol!(c"IOReportSimpleGetIntegerValue"),
            channel_unit: symbol!(c"IOReportChannelGetUnitLabel"),
            state_count: symbol!(c"IOReportStateGetCount"),
            state_name: symbol!(c"IOReportStateGetNameForIndex"),
            state_residency: symbol!(c"IOReportStateGetResidency"),
        })
    }
}

impl Drop for IoReportApi {
    fn drop(&mut self) {
        unsafe { dlclose(self.handle) };
    }
}

unsafe fn dynamic_symbol<T: Copy>(handle: *mut c_void, name: &CStr) -> Option<T> {
    let symbol = unsafe { dlsym(handle, name.as_ptr()) };
    if symbol.is_null() || mem::size_of::<T>() != mem::size_of::<*mut c_void>() {
        None
    } else {
        Some(unsafe { mem::transmute_copy(&symbol) })
    }
}

struct HidThermalApi {
    handle: *mut c_void,
    client_create: HidClientCreate,
    set_matching: HidSetMatching,
    copy_services: HidCopyServices,
    copy_event: HidCopyEvent,
    copy_property: HidCopyProperty,
    event_float: HidEventFloat,
}

impl HidThermalApi {
    fn load() -> Option<Self> {
        let handle = unsafe {
            dlopen(
                c"/System/Library/Frameworks/IOKit.framework/IOKit".as_ptr(),
                RTLD_LAZY | RTLD_LOCAL,
            )
        };
        if handle.is_null() {
            return None;
        }
        macro_rules! symbol {
            ($name:expr) => {
                match unsafe { dynamic_symbol(handle, $name) } {
                    Some(symbol) => symbol,
                    None => {
                        unsafe { dlclose(handle) };
                        return None;
                    }
                }
            };
        }
        Some(Self {
            handle,
            client_create: symbol!(c"IOHIDEventSystemClientCreate"),
            set_matching: symbol!(c"IOHIDEventSystemClientSetMatching"),
            copy_services: symbol!(c"IOHIDEventSystemClientCopyServices"),
            copy_event: symbol!(c"IOHIDServiceClientCopyEvent"),
            copy_property: symbol!(c"IOHIDServiceClientCopyProperty"),
            event_float: symbol!(c"IOHIDEventGetFloatValue"),
        })
    }
}

impl Drop for HidThermalApi {
    fn drop(&mut self) {
        unsafe { dlclose(self.handle) };
    }
}

pub(crate) struct AppleGpuCollector {
    api: IoReportApi,
    subscription: IoReportSubscription,
    channels: CfRef,
    previous: CfRef,
    previous_at: Instant,
    name: String,
    frequencies_mhz: Vec<u32>,
    gpu_service: Option<u32>,
    encoder_service: Option<u32>,
    decoder_service: Option<u32>,
    memory_total: u64,
    smc: Option<AppleSmc>,
}

pub(crate) struct AppleCpuFrequencyCollector {
    api: IoReportApi,
    subscription: IoReportSubscription,
    channels: CfRef,
    previous: CfRef,
    efficiency_frequencies_mhz: Vec<u32>,
    performance_frequencies_mhz: Vec<u32>,
}

impl AppleCpuFrequencyCollector {
    pub(crate) fn new() -> Option<Self> {
        let api = IoReportApi::load()?;
        let group = cf_string("CPU Stats")?;
        let subgroup = cf_string("CPU Core Performance States")?;
        let source = unsafe { (api.copy_channels)(group, subgroup, 0, 0, 0) };
        unsafe {
            CFRelease(group);
            CFRelease(subgroup);
        }
        if unsafe { !cf_is_type(source, CFDictionaryGetTypeID()) } {
            if !source.is_null() {
                unsafe { CFRelease(source) };
            }
            return None;
        }
        let channels = unsafe {
            CFDictionaryCreateMutableCopy(ptr::null(), CFDictionaryGetCount(source), source)
        };
        unsafe { CFRelease(source) };
        if channels.is_null() {
            return None;
        }
        let mut subscribed_channels = ptr::null();
        let subscription = unsafe {
            (api.create_subscription)(
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
        let previous = unsafe { (api.create_samples)(subscription, channels, ptr::null()) };
        if previous.is_null() {
            unsafe {
                CFRelease(subscription);
                CFRelease(channels);
            }
            return None;
        }
        let (efficiency_frequencies_mhz, performance_frequencies_mhz) = cpu_frequencies();
        if efficiency_frequencies_mhz.is_empty() || performance_frequencies_mhz.is_empty() {
            unsafe {
                CFRelease(previous);
                CFRelease(subscription);
                CFRelease(channels);
            }
            return None;
        }
        Some(Self {
            api,
            subscription,
            channels,
            previous,
            efficiency_frequencies_mhz,
            performance_frequencies_mhz,
        })
    }

    pub(crate) fn collect_mhz(&mut self) -> Vec<f64> {
        let current =
            unsafe { (self.api.create_samples)(self.subscription, self.channels, ptr::null()) };
        if current.is_null() {
            return Vec::new();
        }
        let delta = unsafe { (self.api.create_samples_delta)(self.previous, current, ptr::null()) };
        unsafe { CFRelease(self.previous) };
        self.previous = current;
        if unsafe { !cf_is_type(delta, CFDictionaryGetTypeID()) } {
            if !delta.is_null() {
                unsafe { CFRelease(delta) };
            }
            return Vec::new();
        }
        let Some(key) = cf_string("IOReportChannels") else {
            unsafe { CFRelease(delta) };
            return Vec::new();
        };
        let items = unsafe { CFDictionaryGetValue(delta, key) };
        unsafe { CFRelease(key) };
        if unsafe { !cf_is_type(items, CFArrayGetTypeID()) } {
            unsafe { CFRelease(delta) };
            return Vec::new();
        }
        let mut frequencies = Vec::new();
        for index in 0..unsafe { CFArrayGetCount(items) } {
            let item = unsafe { CFArrayGetValueAtIndex(items, index) };
            let channel = cf_string_value(unsafe { (self.api.channel_name)(item) });
            let table = if channel.contains("PCPU") {
                &self.performance_frequencies_mhz
            } else if channel.contains("ECPU") || channel.contains("MCPU") {
                &self.efficiency_frequencies_mhz
            } else {
                continue;
            };
            if let Some(frequency) = residency_weighted_frequency(item, table, &self.api) {
                let tier = usize::from(channel.contains("PCPU"));
                frequencies.push((tier, channel, frequency));
            }
        }
        unsafe { CFRelease(delta) };
        frequencies.sort_unstable_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
        frequencies
            .into_iter()
            .map(|(_, _, frequency)| frequency)
            .collect()
    }
}

impl Drop for AppleCpuFrequencyCollector {
    fn drop(&mut self) {
        unsafe {
            CFRelease(self.previous);
            CFRelease(self.channels);
            CFRelease(self.subscription);
        }
    }
}

#[derive(Default)]
struct GpuDelta {
    utilization: u32,
    memory_utilization: u32,
    gpu_clock_mhz: u32,
    power_mw: u64,
    power_state: i32,
    encoder_power_mw: f64,
    decoder_power_mw: f64,
    encoder_read_bps: u64,
    encoder_write_bps: u64,
    decoder_read_bps: u64,
    decoder_write_bps: u64,
    utilization_supported: bool,
    memory_utilization_supported: bool,
    power_supported: bool,
    encoder_power_supported: bool,
    decoder_power_supported: bool,
    encoder_bandwidth_supported: bool,
    decoder_bandwidth_supported: bool,
}

impl AppleGpuCollector {
    pub(crate) fn new() -> Option<Self> {
        let api = IoReportApi::load()?;
        let gpu_group = cf_string("GPU Stats")?;
        let gpu_subgroup = cf_string("GPU Performance States")?;
        let energy_group = cf_string("Energy Model")?;
        let memory_group = cf_string("AMC Stats")?;
        let memory_subgroup = cf_string("Perf Counters")?;
        let bandwidth_group = cf_string("PMP")?;
        let bandwidth_subgroup = cf_string("DCS BW")?;
        let gpu_channels = unsafe { (api.copy_channels)(gpu_group, gpu_subgroup, 0, 0, 0) };
        let energy_channels = unsafe { (api.copy_channels)(energy_group, ptr::null(), 0, 0, 0) };
        let memory_channels =
            unsafe { (api.copy_channels)(memory_group, memory_subgroup, 0, 0, 0) };
        let bandwidth_channels =
            unsafe { (api.copy_channels)(bandwidth_group, bandwidth_subgroup, 0, 0, 0) };
        unsafe {
            CFRelease(gpu_group);
            CFRelease(gpu_subgroup);
            CFRelease(energy_group);
            CFRelease(memory_group);
            CFRelease(memory_subgroup);
            CFRelease(bandwidth_group);
            CFRelease(bandwidth_subgroup);
        }
        let channel_sets = [
            gpu_channels,
            energy_channels,
            memory_channels,
            bandwidth_channels,
        ];
        let &base = channel_sets.iter().find(|channels| !channels.is_null())?;
        for &channels in &channel_sets {
            if !channels.is_null() && channels != base {
                unsafe { (api.merge_channels)(base, channels, ptr::null()) };
            }
        }
        if unsafe { !cf_is_type(base, CFDictionaryGetTypeID()) } {
            for channels in channel_sets {
                if !channels.is_null() {
                    unsafe { CFRelease(channels) };
                }
            }
            return None;
        }
        let channels =
            unsafe { CFDictionaryCreateMutableCopy(ptr::null(), CFDictionaryGetCount(base), base) };
        for channel_set in channel_sets {
            if !channel_set.is_null() {
                unsafe { CFRelease(channel_set) };
            }
        }
        if channels.is_null() {
            return None;
        }
        let mut subscribed_channels = ptr::null();
        let subscription = unsafe {
            (api.create_subscription)(
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
        let previous = unsafe { (api.create_samples)(subscription, channels, ptr::null()) };
        let chip = sysctl_string("machdep.cpu.brand_string")
            .unwrap_or_else(|| "Apple Silicon".to_string());
        let gpu_service = find_service("AGXAccelerator");
        let core_count = gpu_service.and_then(|service| registry_u64(service, "gpu-core-count"));
        let name = if let Some(cores) = core_count.filter(|cores| *cores > 0) {
            format!("{} {cores}-core GPU", chip.trim())
        } else {
            format!("{} GPU", chip.trim())
        };
        Some(Self {
            api,
            subscription,
            channels,
            previous,
            previous_at: Instant::now(),
            name,
            frequencies_mhz: gpu_frequencies(),
            gpu_service,
            encoder_service: find_service("AppleAVE2Driver").or_else(|| find_service("AppleAVE")),
            decoder_service: find_service("AppleAVD"),
            memory_total: sysctl_u64("hw.memsize").unwrap_or(0),
            smc: AppleSmc::new(),
        })
    }

    pub(crate) fn collect(&mut self, check_temperature: bool) -> GpuSample {
        let elapsed = self.previous_at.elapsed().as_secs_f64().max(0.001);
        self.previous_at = Instant::now();
        let current =
            unsafe { (self.api.create_samples)(self.subscription, self.channels, ptr::null()) };
        let delta = if current.is_null() || self.previous.is_null() {
            ptr::null()
        } else {
            unsafe { (self.api.create_samples_delta)(self.previous, current, ptr::null()) }
        };
        if !self.previous.is_null() {
            unsafe { CFRelease(self.previous) };
        }
        self.previous = current;

        let mut sample = GpuDelta::default();
        if !delta.is_null() {
            self.parse_delta(delta, elapsed, &mut sample);
            unsafe { CFRelease(delta) };
        }
        let temperature = check_temperature
            .then(|| {
                read_gpu_temperature().or_else(|| {
                    self.smc
                        .as_ref()
                        .and_then(AppleSmc::read_a18_gpu_temperature)
                })
            })
            .flatten();
        let memory_used = self.gpu_service.and_then(read_agx_memory_used);
        let encoder_sessions = self.encoder_service.map(|service| {
            let ave2 = count_children_of_class(service, "AppleAVE2UserClient");
            if ave2 > 0 {
                ave2
            } else {
                count_children_of_class(service, "AppleAVEUserClient")
            }
        });
        let decoder_sessions = self
            .decoder_service
            .map(|service| count_children_of_class(service, "AppleAVDUserClient"));

        GpuSample {
            name: self.name.clone(),
            utilization: sample.utilization,
            memory_utilization: sample.memory_utilization,
            gpu_clock_mhz: sample.gpu_clock_mhz,
            power_mw: sample.power_mw,
            // Apple exposes estimated energy but no GPU power-limit value.
            // Avoid presenting the generic 255 W default as an Apple limit.
            power_limit_mw: 0,
            power_state: sample.power_state,
            temperature_c: temperature.unwrap_or(0.0).round() as i64,
            temperature_max_c: 110,
            memory_total: self.memory_total,
            memory_used: memory_used
                .map(|used| {
                    if self.memory_total > 0 {
                        used.min(self.memory_total)
                    } else {
                        used
                    }
                })
                .unwrap_or(0),
            encoder_power_mw: sample.encoder_power_mw,
            decoder_power_mw: sample.decoder_power_mw,
            encoder_read_bps: sample.encoder_read_bps,
            encoder_write_bps: sample.encoder_write_bps,
            decoder_read_bps: sample.decoder_read_bps,
            decoder_write_bps: sample.decoder_write_bps,
            encoder_sessions: encoder_sessions.unwrap_or(0),
            decoder_sessions: decoder_sessions.unwrap_or(0),
            support: GpuSupport {
                utilization: sample.utilization_supported,
                memory_utilization: sample.memory_utilization_supported,
                gpu_clock: sample.utilization_supported && !self.frequencies_mhz.is_empty(),
                power: sample.power_supported,
                power_state: sample.utilization_supported,
                temperature: temperature.is_some(),
                memory_total: self.memory_total > 0,
                memory_used: memory_used.is_some(),
                unified_memory: true,
                // Some macOS 27 Apple-silicon systems advertise AVE/VDEC
                // energy channels but leave them frozen at zero. Do not
                // present that unavailable reading as measured power.
                encoder_power: sample.encoder_power_supported && sample.encoder_power_mw > 0.0,
                decoder_power: sample.decoder_power_supported && sample.decoder_power_mw > 0.0,
                encoder_bandwidth: sample.encoder_bandwidth_supported
                    && sample
                        .encoder_read_bps
                        .saturating_add(sample.encoder_write_bps)
                        > 0,
                decoder_bandwidth: sample.decoder_bandwidth_supported
                    && sample
                        .decoder_read_bps
                        .saturating_add(sample.decoder_write_bps)
                        > 0,
                encoder_sessions: encoder_sessions.is_some(),
                decoder_sessions: decoder_sessions.is_some(),
                ..GpuSupport::default()
            },
            ..GpuSample::default()
        }
    }

    fn parse_delta(&self, delta: CfRef, elapsed: f64, sample: &mut GpuDelta) {
        if unsafe { !cf_is_type(delta, CFDictionaryGetTypeID()) } {
            return;
        }
        let Some(key) = cf_string("IOReportChannels") else {
            return;
        };
        let channel_array = unsafe { CFDictionaryGetValue(delta, key) };
        unsafe { CFRelease(key) };
        if unsafe { !cf_is_type(channel_array, CFArrayGetTypeID()) } {
            return;
        }
        let count = unsafe { CFArrayGetCount(channel_array) };
        for index in 0..count {
            let item = unsafe { CFArrayGetValueAtIndex(channel_array, index) };
            if item.is_null() {
                continue;
            }
            let group = cf_string_value(unsafe { (self.api.channel_group)(item) });
            let subgroup = cf_string_value(unsafe { (self.api.channel_subgroup)(item) });
            let channel = cf_string_value(unsafe { (self.api.channel_name)(item) });
            if group == "GPU Stats" && subgroup == "GPU Performance States" && channel == "GPUPH" {
                let states = unsafe { (self.api.state_count)(item) };
                let mut total = 0_i64;
                let mut active_start = 0;
                for state in 0..states {
                    let name = cf_string_value(unsafe { (self.api.state_name)(item, state) });
                    if matches!(name.as_str(), "IDLE" | "OFF" | "DOWN") {
                        active_start = state + 1;
                    }
                    let residency = unsafe { (self.api.state_residency)(item, state) }.max(0);
                    total = total.saturating_add(residency);
                }
                let active = (active_start..states).fold(0_i64, |sum, state| {
                    sum.saturating_add(unsafe { (self.api.state_residency)(item, state) }.max(0))
                });
                sample.power_state = (active_start..states)
                    .max_by_key(|state| unsafe { (self.api.state_residency)(item, *state).max(0) })
                    .filter(|state| unsafe { (self.api.state_residency)(item, *state) } > 0)
                    .map(|state| {
                        let name = cf_string_value(unsafe { (self.api.state_name)(item, state) });
                        name.strip_prefix('P')
                            .and_then(|value| value.parse().ok())
                            .unwrap_or(state)
                    })
                    .unwrap_or(32);
                if total > 0 {
                    sample.utilization =
                        ((active as f64 * 100.0 / total as f64).round() as u32).min(100);
                    sample.utilization_supported = true;
                }
                if active > 0 && !self.frequencies_mhz.is_empty() {
                    let weighted = (active_start..states)
                        .zip(self.frequencies_mhz.iter().copied())
                        .fold(0_f64, |sum, (state, frequency)| {
                            let residency =
                                unsafe { (self.api.state_residency)(item, state) }.max(0) as f64;
                            sum + residency * f64::from(frequency)
                        });
                    sample.gpu_clock_mhz = (weighted / active as f64).round() as u32;
                }
            } else if group == "PMP" && subgroup == "DCS BW" && channel == "AGX RD+WR" {
                // This state histogram records time in bandwidth buckets
                // (1GB/s through the reporter's maximum bucket). Normalize
                // the residency-weighted bandwidth to that reported range;
                // it is memory activity, distinct from UMA occupancy.
                let states = unsafe { (self.api.state_count)(item) };
                let mut total_residency = 0_u128;
                let mut weighted_bandwidth = 0_u128;
                let mut maximum_bandwidth = 0_u64;
                for state in 0..states {
                    let name = cf_string_value(unsafe { (self.api.state_name)(item, state) });
                    let Some(bandwidth) = bandwidth_state_value(&name) else {
                        continue;
                    };
                    let residency =
                        unsafe { (self.api.state_residency)(item, state) }.max(0) as u128;
                    total_residency = total_residency.saturating_add(residency);
                    weighted_bandwidth = weighted_bandwidth
                        .saturating_add(residency.saturating_mul(u128::from(bandwidth)));
                    maximum_bandwidth = maximum_bandwidth.max(bandwidth);
                }
                if let Some(utilization) = normalized_bandwidth_utilization(
                    total_residency,
                    weighted_bandwidth,
                    maximum_bandwidth,
                ) {
                    sample.memory_utilization = utilization;
                    sample.memory_utilization_supported = true;
                }
            } else if group == "Energy Model"
                && matches!(channel.as_str(), "GPU Energy" | "AVE" | "VDEC")
            {
                let unit = cf_string_value(unsafe { (self.api.channel_unit)(item) });
                let value = unsafe { (self.api.simple_integer)(item, 0) }.max(0) as u64;
                let power_mw = energy_delta_to_power_mw(value, &unit, elapsed);
                match channel.as_str() {
                    "GPU Energy" => {
                        sample.power_mw = power_mw.round() as u64;
                        sample.power_supported = true;
                    }
                    "AVE" => {
                        sample.encoder_power_mw = power_mw;
                        sample.encoder_power_supported = true;
                    }
                    "VDEC" => {
                        sample.decoder_power_mw = power_mw;
                        sample.decoder_power_supported = true;
                    }
                    _ => {}
                }
            } else if group == "AMC Stats" && subgroup == "Perf Counters" {
                let value = unsafe { (self.api.simple_integer)(item, 0) }.max(0) as u64;
                let bytes_per_second = (value as f64 / elapsed).round().max(0.0) as u64;
                let is_encoder = channel.starts_with("AVE");
                let is_decoder = channel.starts_with("AVD") || channel.starts_with("VDEC");
                // Prefer the DCS observation point over AF so the same memory
                // traffic is not counted twice at two interconnect stages.
                if channel.ends_with(" DCS RD") {
                    if is_encoder {
                        sample.encoder_read_bps =
                            sample.encoder_read_bps.saturating_add(bytes_per_second);
                        sample.encoder_bandwidth_supported = true;
                    } else if is_decoder {
                        sample.decoder_read_bps =
                            sample.decoder_read_bps.saturating_add(bytes_per_second);
                        sample.decoder_bandwidth_supported = true;
                    }
                } else if channel.ends_with(" DCS WR") {
                    if is_encoder {
                        sample.encoder_write_bps =
                            sample.encoder_write_bps.saturating_add(bytes_per_second);
                        sample.encoder_bandwidth_supported = true;
                    } else if is_decoder {
                        sample.decoder_write_bps =
                            sample.decoder_write_bps.saturating_add(bytes_per_second);
                        sample.decoder_bandwidth_supported = true;
                    }
                }
            }
        }
    }
}

fn energy_delta_to_power_mw(value: u64, unit: &str, elapsed: f64) -> f64 {
    let joules = if unit.contains("nJ") {
        value as f64 / 1_000_000_000.0
    } else if unit.contains("uJ") || unit.contains("µJ") {
        value as f64 / 1_000_000.0
    } else if unit.contains("mJ") {
        value as f64 / 1_000.0
    } else {
        value as f64
    };
    (joules * 1_000.0 / elapsed.max(0.001)).max(0.0)
}

fn bandwidth_state_value(label: &str) -> Option<u64> {
    let label = label.trim();
    for (suffix, multiplier) in [
        ("TB/s", 1_000_000_000_000_u64),
        ("GB/s", 1_000_000_000_u64),
        ("MB/s", 1_000_000_u64),
        ("KB/s", 1_000_u64),
    ] {
        if let Some(value) = label.strip_suffix(suffix) {
            let value = value.trim().parse::<u64>().ok()?;
            return Some(value.saturating_mul(multiplier));
        }
    }
    None
}

fn normalized_bandwidth_utilization(
    total_residency: u128,
    weighted_bandwidth: u128,
    maximum_bandwidth: u64,
) -> Option<u32> {
    if total_residency == 0 || maximum_bandwidth == 0 {
        return None;
    }
    let maximum_weighted = total_residency.saturating_mul(u128::from(maximum_bandwidth));
    Some(
        ((weighted_bandwidth.saturating_mul(100) + maximum_weighted / 2) / maximum_weighted)
            .min(100) as u32,
    )
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
            if let Some(service) = self.gpu_service {
                IOObjectRelease(service);
            }
            if let Some(service) = self.encoder_service {
                IOObjectRelease(service);
            }
            if let Some(service) = self.decoder_service {
                IOObjectRelease(service);
            }
        }
    }
}

fn find_service(class: &str) -> Option<u32> {
    let class = CString::new(class).ok()?;
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null() {
        return None;
    }
    let mut iterator = 0_u32;
    if unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0 {
        return None;
    }
    let service = unsafe { IOIteratorNext(iterator) };
    unsafe { IOObjectRelease(iterator) };
    (service != 0).then_some(service)
}

fn count_children_of_class(entry: u32, class: &str) -> u32 {
    let Ok(class) = CString::new(class) else {
        return 0;
    };
    let mut iterator = 0_u32;
    if unsafe { IORegistryEntryGetChildIterator(entry, c"IOService".as_ptr(), &mut iterator) } != 0
    {
        return 0;
    }
    let mut count = 0_u32;
    loop {
        let child = unsafe { IOIteratorNext(iterator) };
        if child == 0 {
            break;
        }
        if unsafe { IOObjectConformsTo(child, class.as_ptr()) } != 0 {
            count = count.saturating_add(1);
        }
        unsafe { IOObjectRelease(child) };
    }
    unsafe { IOObjectRelease(iterator) };
    count
}

fn registry_u64(entry: u32, key: &str) -> Option<u64> {
    let key = cf_string(key)?;
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, key, ptr::null(), 0) };
    unsafe { CFRelease(key) };
    let result = cf_u64(value);
    if !value.is_null() {
        unsafe { CFRelease(value) };
    }
    result
}

fn read_agx_memory_used(service: u32) -> Option<u64> {
    let key = cf_string("PerformanceStatistics")?;
    let statistics = unsafe { IORegistryEntryCreateCFProperty(service, key, ptr::null(), 0) };
    unsafe { CFRelease(key) };
    if unsafe { !cf_is_type(statistics, CFDictionaryGetTypeID()) } {
        if !statistics.is_null() {
            unsafe { CFRelease(statistics) };
        }
        return None;
    }
    let Some(used_key) = cf_string("In use system memory") else {
        unsafe { CFRelease(statistics) };
        return None;
    };
    let used = unsafe { CFDictionaryGetValue(statistics, used_key) };
    let result = cf_u64(used);
    unsafe {
        CFRelease(used_key);
        CFRelease(statistics);
    }
    result
}

fn cf_u64(value: CfRef) -> Option<u64> {
    if unsafe { !cf_is_type(value, CFNumberGetTypeID()) } {
        return None;
    }
    let mut number = 0_i64;
    if unsafe { CFNumberGetValue(value, CF_NUMBER_SINT64, (&mut number as *mut i64).cast()) }
        && number >= 0
    {
        Some(number as u64)
    } else {
        None
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
        let Some(api) = HidThermalApi::load() else {
            return Vec::new();
        };
        let matching = thermal_matching_dictionary();
        if matching.is_null() {
            return Vec::new();
        }
        let client = (api.client_create)(ptr::null());
        if client.is_null() {
            CFRelease(matching);
            return Vec::new();
        }
        (api.set_matching)(client, matching);
        let services = (api.copy_services)(client);
        let mut result = Vec::new();
        if !services.is_null()
            && cf_is_type(services, CFArrayGetTypeID())
            && let Some(product_key) = cf_string("Product")
        {
            for index in 0..CFArrayGetCount(services) {
                let service = CFArrayGetValueAtIndex(services, index);
                if service.is_null() {
                    continue;
                }
                let property = (api.copy_property)(service, product_key);
                let event = (api.copy_event)(service, HID_EVENT_TEMPERATURE, 0, 0);
                if !property.is_null() && !event.is_null() {
                    let name = cf_string_value(property);
                    let value = (api.event_float)(event, (HID_EVENT_TEMPERATURE << 16) as i32);
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
            CFRelease(product_key);
        }
        if !services.is_null() {
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

fn residency_weighted_frequency(
    item: CfRef,
    frequencies_mhz: &[u32],
    api: &IoReportApi,
) -> Option<f64> {
    let states = unsafe { (api.state_count)(item) }.max(0);
    let active_start = (0..states).find(|state| {
        let name = cf_string_value(unsafe { (api.state_name)(item, *state) });
        !matches!(name.as_str(), "IDLE" | "DOWN" | "OFF")
    })?;
    let count = (states - active_start).min(frequencies_mhz.len() as i32);
    if count <= 0 {
        return None;
    }
    let residencies = (0..count)
        .map(|offset| unsafe { (api.state_residency)(item, active_start + offset) }.max(0) as u64)
        .collect::<Vec<_>>();
    weighted_frequency_from_residencies(&residencies, frequencies_mhz)
}

fn weighted_frequency_from_residencies(
    residencies: &[u64],
    frequencies_mhz: &[u32],
) -> Option<f64> {
    let minimum = frequencies_mhz
        .iter()
        .copied()
        .find(|frequency| *frequency > 0)?;
    let (active, weighted) = residencies.iter().zip(frequencies_mhz).fold(
        (0_u128, 0_u128),
        |(active, weighted), (residency, frequency)| {
            let residency = u128::from(*residency);
            (
                active.saturating_add(residency),
                weighted.saturating_add(residency.saturating_mul(u128::from(*frequency))),
            )
        },
    );
    Some(if active > 0 && weighted > 0 {
        weighted as f64 / active as f64
    } else {
        f64::from(minimum)
    })
}

fn cpu_frequencies() -> (Vec<u32>, Vec<u32>) {
    let Ok(class) = CString::new("AppleARMIODevice") else {
        return (Vec::new(), Vec::new());
    };
    let mut iterator = 0_u32;
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    if matching.is_null()
        || unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != 0
    {
        return (Vec::new(), Vec::new());
    }
    let mut tables = (Vec::new(), Vec::new());
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
                && unsafe { cf_is_type(properties, CFDictionaryGetTypeID()) }
            {
                let keys = cpu_frequency_property_keys(properties).unwrap_or_else(|| {
                    ("voltage-states1-sram".into(), "voltage-states5-sram".into())
                });
                tables.0 = dvfs_frequencies(properties, &keys.0);
                tables.1 = dvfs_frequencies(properties, &keys.1);
            }
            if !properties.is_null() {
                unsafe { CFRelease(properties) };
            }
        }
        unsafe { IOObjectRelease(entry) };
        if !tables.0.is_empty() && !tables.1.is_empty() {
            break;
        }
    }
    unsafe { IOObjectRelease(iterator) };
    tables
}

fn cpu_frequency_property_keys(properties: CfRef) -> Option<(String, String)> {
    if property_data(properties, "voltage-states1-sram").is_some()
        && property_data(properties, "voltage-states5-sram").is_some()
    {
        return Some(("voltage-states1-sram".into(), "voltage-states5-sram".into()));
    }
    cpu_frequency_keys_from_clusters(&property_data(properties, "acc-clusters")?)
}

fn cpu_frequency_keys_from_clusters(data: &[u8]) -> Option<(String, String)> {
    let mut clusters = data
        .as_chunks::<8>()
        .0
        .iter()
        .map(|entry| (entry[1], entry[0]))
        .collect::<Vec<_>>();
    clusters.sort_unstable();
    let efficiency = clusters.get(clusters.len().checked_sub(2)?)?.1;
    let performance = clusters.last()?.1;
    Some((
        format!("voltage-states{efficiency}-sram"),
        format!("voltage-states{performance}-sram"),
    ))
}

fn dvfs_frequencies(properties: CfRef, name: &str) -> Vec<u32> {
    let Some(data) = property_data(properties, name) else {
        return Vec::new();
    };
    let raw = data
        .as_chunks::<8>()
        .0
        .iter()
        .map(|entry| u32::from_le_bytes(entry[..4].try_into().unwrap()))
        .collect::<Vec<_>>();
    normalize_dvfs_frequencies(raw)
}

fn normalize_dvfs_frequencies(raw: Vec<u32>) -> Vec<u32> {
    let maximum = raw.iter().copied().max().unwrap_or(0);
    let scale = if maximum >= 100_000_000 {
        1_000_000
    } else {
        1_000
    };
    raw.into_iter().map(|frequency| frequency / scale).collect()
}

fn property_data(properties: CfRef, name: &str) -> Option<Vec<u8>> {
    let key = cf_string(name)?;
    let data = unsafe { CFDictionaryGetValue(properties, key) };
    unsafe { CFRelease(key) };
    if unsafe { !cf_is_type(data, CFDataGetTypeID()) } {
        return None;
    }
    let length = unsafe { CFDataGetLength(data) }.max(0) as usize;
    let bytes = unsafe { CFDataGetBytePtr(data) };
    (!bytes.is_null()).then(|| unsafe { std::slice::from_raw_parts(bytes, length) }.to_vec())
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
                if unsafe { cf_is_type(properties, CFDictionaryGetTypeID()) }
                    && let Some(key) = cf_string("voltage-states9")
                {
                    let data = unsafe { CFDictionaryGetValue(properties, key) };
                    if unsafe { cf_is_type(data, CFDataGetTypeID()) } {
                        let length = unsafe { CFDataGetLength(data) }.max(0) as usize;
                        let bytes = unsafe { CFDataGetBytePtr(data) };
                        if !bytes.is_null() {
                            let values = unsafe { std::slice::from_raw_parts(bytes, length) };
                            for pair in values.as_chunks::<8>().0 {
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
    } == 0
        && size == mem::size_of::<u64>())
    .then_some(value)
}

#[cfg(test)]
fn sysctl_u32(name: &str) -> Option<u32> {
    let name = CString::new(name).ok()?;
    let mut value = 0_u32;
    let mut size = mem::size_of::<u32>();
    (unsafe {
        sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut u32).cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } == 0
        && size == mem::size_of::<u32>())
    .then_some(value)
}

fn cf_string(value: &str) -> Option<CfRef> {
    let value = CString::new(value).ok()?;
    let string =
        unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), CF_STRING_ENCODING_UTF8) };
    (!string.is_null()).then_some(string)
}

fn cf_string_value(value: CfRef) -> String {
    if unsafe { !cf_is_type(value, CFStringGetTypeID()) } {
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

unsafe fn cf_is_type(value: CfRef, expected: usize) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) == expected }
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

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> CfRef;
    fn IOServiceGetMatchingServices(main_port: u32, matching: CfRef, iterator: *mut u32) -> c_int;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectConformsTo(object: u32, class: *const c_char) -> u32;
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
    fn IORegistryEntryGetChildIterator(
        entry: u32,
        plane: *const c_char,
        iterator: *mut u32,
    ) -> c_int;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut CfRef,
        allocator: CfRef,
        options: u32,
    ) -> c_int;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: CfRef,
        allocator: CfRef,
        options: u32,
    ) -> CfRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeDictionaryKeyCallBacks: CfDictionaryKeyCallbacks;
    static kCFTypeDictionaryValueCallBacks: CfDictionaryValueCallbacks;
    fn CFRelease(value: CfRef);
    fn CFGetTypeID(value: CfRef) -> usize;
    fn CFArrayGetTypeID() -> usize;
    fn CFDataGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringCreateWithCString(allocator: CfRef, value: *const c_char, encoding: u32) -> CfRef;
    fn CFStringGetCString(
        string: CfRef,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFNumberCreate(allocator: CfRef, kind: c_int, value: *const c_void) -> CfRef;
    fn CFNumberGetValue(number: CfRef, kind: c_int, value: *mut c_void) -> bool;
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
    fn dlopen(path: *const c_char, mode: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
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
    fn apple_cpu_frequency_tables_handle_hz_khz_and_dynamic_cluster_keys() {
        assert_eq!(
            normalize_dvfs_frequencies(vec![744_000_000, 1_020_000_000]),
            vec![744, 1_020]
        );
        assert_eq!(
            normalize_dvfs_frequencies(vec![744_000, 1_020_000]),
            vec![744, 1_020]
        );
        let mut clusters = Vec::new();
        clusters.extend_from_slice(&[23, 1, 0, 0, 0, 0, 0, 0]);
        clusters.extend_from_slice(&[5, 2, 0, 0, 0, 0, 0, 0]);
        assert_eq!(
            cpu_frequency_keys_from_clusters(&clusters),
            Some((
                "voltage-states23-sram".into(),
                "voltage-states5-sram".into()
            ))
        );
        assert_eq!(
            weighted_frequency_from_residencies(&[1, 3], &[600, 1_200]),
            Some(1_050.0)
        );
        assert_eq!(
            weighted_frequency_from_residencies(&[0, 0], &[600, 1_200]),
            Some(600.0)
        );
    }

    #[test]
    #[ignore = "requires live Apple-silicon IOReport and IORegistry data"]
    fn collects_live_apple_cpu_frequency() {
        let mut collector = AppleCpuFrequencyCollector::new().expect("CPU frequency support");
        std::thread::sleep(std::time::Duration::from_millis(100));
        let frequencies = collector.collect_mhz();
        assert!(!frequencies.is_empty());
        assert_eq!(
            frequencies.len(),
            sysctl_u32("hw.physicalcpu").expect("physical CPU count") as usize
        );
        assert!(
            frequencies
                .iter()
                .all(|frequency| (100.0..10_000.0).contains(frequency))
        );
    }

    #[test]
    fn apple_smc_layout_and_a18_float_temperature_match_the_native_abi() {
        assert_eq!(mem::size_of::<SmcKeyData>(), 80);
        let mut bytes = [0_u8; 32];
        bytes[..4].copy_from_slice(&[0x7e, 0xc1, 0x7a, 0x42]);
        let value = decode_smc_temperature(4, u32::from_be_bytes(*b"flt "), &bytes).unwrap();
        assert!((value - 62.688_957).abs() < 0.000_1);
    }

    #[test]
    fn io_report_energy_units_convert_to_average_milliwatts() {
        assert_eq!(energy_delta_to_power_mw(500_000_000, "nJ", 1.0), 500.0);
        assert_eq!(energy_delta_to_power_mw(500_000, "uJ", 1.0), 500.0);
        assert_eq!(energy_delta_to_power_mw(500, "mJ", 1.0), 500.0);
        assert_eq!(energy_delta_to_power_mw(375, "nJ", 1.0), 0.000_375);
    }

    #[test]
    fn gpu_memory_bandwidth_histogram_is_normalized_to_its_reported_range() {
        assert_eq!(bandwidth_state_value("  32GB/s"), Some(32_000_000_000));
        assert_eq!(bandwidth_state_value("500MB/s"), Some(500_000_000));
        assert_eq!(normalized_bandwidth_utilization(2, 5, 4), Some(63));
        assert_eq!(normalized_bandwidth_utilization(0, 0, 4), None);
    }
}
