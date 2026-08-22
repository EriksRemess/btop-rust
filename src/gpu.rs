use std::ffi::{CStr, CString, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::fs;
use std::mem;
use std::os::fd::RawFd;
use std::path::{Path, PathBuf};
use std::ptr;
use std::time::{Duration, Instant};

use crate::config::Config;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
pub(crate) mod macos;

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
    pub pcie: bool,
    pub encoder: bool,
    pub decoder: bool,
}

#[derive(Debug, Clone)]
pub struct GpuSample {
    pub name: String,
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
    pub support: GpuSupport,
}

impl Default for GpuSample {
    fn default() -> Self {
        Self {
            name: String::new(),
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
            support: GpuSupport::default(),
        }
    }
}

pub struct GpuCollector {
    nvml: Option<Nvml>,
    rsmi: Option<Rsmi>,
    amd_sysfs: Vec<AmdSysfsDevice>,
    intel: Option<IntelPmu>,
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    apple: Option<macos::AppleGpuCollector>,
    shown: String,
    next_probe: Instant,
}

impl GpuCollector {
    pub fn new(config: &Config) -> Self {
        let shown = config
            .value("shown_gpus")
            .unwrap_or("nvidia amd intel apple")
            .to_string();
        let nvml = shown.contains("nvidia").then(Nvml::load).flatten();
        let rsmi = shown.contains("amd").then(Rsmi::load).flatten();
        let amd_sysfs = if rsmi.is_none() {
            if shown.contains("amd") {
                discover_amd_sysfs()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        let intel = shown.contains("intel").then(IntelPmu::load).flatten();
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        let apple = shown
            .contains("apple")
            .then(macos::AppleGpuCollector::new)
            .flatten();
        Self {
            nvml,
            rsmi,
            amd_sysfs,
            intel,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            apple,
            shown,
            next_probe: Instant::now() + Duration::from_secs(10),
        }
    }

    pub fn collect(&mut self, config: &Config) -> Vec<GpuSample> {
        let shown = config
            .value("shown_gpus")
            .unwrap_or("nvidia amd intel apple");
        let missing_requested = (shown.contains("nvidia") && self.nvml.is_none())
            || (shown.contains("amd") && self.rsmi.is_none() && self.amd_sysfs.is_empty())
            || (shown.contains("intel") && self.intel.is_none())
            || {
                #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
                {
                    shown.contains("apple") && self.apple.is_none()
                }
                #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
                {
                    false
                }
            };
        if shown != self.shown || (missing_requested && Instant::now() >= self.next_probe) {
            *self = Self::new(config);
        }
        let check_temperature = config.check_temperature;
        let nvml_pcie = config
            .bool_value("nvml_measure_pcie_speeds")
            .unwrap_or(true);
        let rsmi_pcie = config
            .bool_value("rsmi_measure_pcie_speeds")
            .unwrap_or(true);
        let mut samples = Vec::new();
        if let Some(nvml) = &self.nvml {
            samples.extend(nvml.collect(check_temperature, nvml_pcie));
        }
        if let Some(rsmi) = &self.rsmi {
            samples.extend(rsmi.collect(check_temperature, rsmi_pcie));
        }
        samples.extend(self.amd_sysfs.iter_mut().map(AmdSysfsDevice::collect));
        if let Some(intel) = &mut self.intel {
            samples.push(intel.collect());
        }
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        if let Some(apple) = &mut self.apple {
            samples.push(apple.collect(check_temperature));
        }
        samples
    }
}

struct DynamicLibrary {
    handle: *mut c_void,
}

impl DynamicLibrary {
    fn open(names: &[&str]) -> Option<Self> {
        for name in names {
            let Ok(name) = CString::new(*name) else {
                continue;
            };
            let handle = unsafe { dlopen(name.as_ptr(), RTLD_LAZY) };
            if !handle.is_null() {
                return Some(Self { handle });
            }
        }
        None
    }

    fn symbol<T: Copy>(&self, name: &'static [u8]) -> Option<T> {
        let pointer = unsafe { dlsym(self.handle, name.as_ptr().cast()) };
        if pointer.is_null() {
            None
        } else {
            debug_assert_eq!(mem::size_of::<T>(), mem::size_of_val(&pointer));
            Some(unsafe { mem::transmute_copy(&pointer) })
        }
    }
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            unsafe { dlclose(self.handle) };
        }
    }
}

type NvmlDevice = *mut c_void;
type NvmlReturn = c_int;
type NvmlInit = unsafe extern "C" fn() -> NvmlReturn;
type NvmlShutdown = unsafe extern "C" fn() -> NvmlReturn;
type NvmlDeviceGetCount = unsafe extern "C" fn(*mut c_uint) -> NvmlReturn;
type NvmlDeviceGetHandle = unsafe extern "C" fn(c_uint, *mut NvmlDevice) -> NvmlReturn;
type NvmlDeviceGetName = unsafe extern "C" fn(NvmlDevice, *mut c_char, c_uint) -> NvmlReturn;
type NvmlDeviceGetUint = unsafe extern "C" fn(NvmlDevice, *mut c_uint) -> NvmlReturn;
type NvmlDeviceGetKindUint = unsafe extern "C" fn(NvmlDevice, c_int, *mut c_uint) -> NvmlReturn;
type NvmlDeviceGetUtilization =
    unsafe extern "C" fn(NvmlDevice, *mut NvmlUtilization) -> NvmlReturn;
type NvmlDeviceGetMemory = unsafe extern "C" fn(NvmlDevice, *mut NvmlMemory) -> NvmlReturn;
type NvmlDeviceGetCodec = unsafe extern "C" fn(NvmlDevice, *mut c_uint, *mut c_uint) -> NvmlReturn;

#[repr(C)]
#[derive(Default)]
struct NvmlUtilization {
    gpu: c_uint,
    memory: c_uint,
}

#[repr(C)]
#[derive(Default)]
struct NvmlMemory {
    total: u64,
    free: u64,
    used: u64,
}

struct NvmlFunctions {
    shutdown: NvmlShutdown,
    get_name: NvmlDeviceGetName,
    get_power_limit: NvmlDeviceGetUint,
    get_temperature_threshold: NvmlDeviceGetKindUint,
    get_utilization: NvmlDeviceGetUtilization,
    get_clock: NvmlDeviceGetKindUint,
    get_power: NvmlDeviceGetUint,
    get_power_state: NvmlDeviceGetUint,
    get_temperature: NvmlDeviceGetKindUint,
    get_memory: NvmlDeviceGetMemory,
    get_pcie: NvmlDeviceGetKindUint,
    get_encoder: NvmlDeviceGetCodec,
    get_decoder: NvmlDeviceGetCodec,
}

struct Nvml {
    _library: DynamicLibrary,
    functions: NvmlFunctions,
    devices: Vec<NvmlDevice>,
    names: Vec<String>,
    power_limits: Vec<u64>,
    temperature_limits: Vec<i64>,
    supports: Vec<GpuSupport>,
}

impl Nvml {
    fn load() -> Option<Self> {
        let library = DynamicLibrary::open(&["libnvidia-ml.so", "libnvidia-ml.so.1"])?;
        let init: NvmlInit = library.symbol(b"nvmlInit\0")?;
        let shutdown = library.symbol(b"nvmlShutdown\0")?;
        let get_count: NvmlDeviceGetCount = library.symbol(b"nvmlDeviceGetCount\0")?;
        let get_handle: NvmlDeviceGetHandle = library.symbol(b"nvmlDeviceGetHandleByIndex\0")?;
        let functions = NvmlFunctions {
            shutdown,
            get_name: library.symbol(b"nvmlDeviceGetName\0")?,
            get_power_limit: library.symbol(b"nvmlDeviceGetPowerManagementLimit\0")?,
            get_temperature_threshold: library.symbol(b"nvmlDeviceGetTemperatureThreshold\0")?,
            get_utilization: library.symbol(b"nvmlDeviceGetUtilizationRates\0")?,
            get_clock: library.symbol(b"nvmlDeviceGetClockInfo\0")?,
            get_power: library.symbol(b"nvmlDeviceGetPowerUsage\0")?,
            get_power_state: library.symbol(b"nvmlDeviceGetPowerState\0")?,
            get_temperature: library.symbol(b"nvmlDeviceGetTemperature\0")?,
            get_memory: library.symbol(b"nvmlDeviceGetMemoryInfo\0")?,
            get_pcie: library.symbol(b"nvmlDeviceGetPcieThroughput\0")?,
            get_encoder: library.symbol(b"nvmlDeviceGetEncoderUtilization\0")?,
            get_decoder: library.symbol(b"nvmlDeviceGetDecoderUtilization\0")?,
        };
        if unsafe { init() } != NVML_SUCCESS {
            return None;
        }
        let mut count = 0;
        if unsafe { get_count(&mut count) } != NVML_SUCCESS || count == 0 {
            unsafe { shutdown() };
            return None;
        }
        let mut devices = Vec::with_capacity(count as usize);
        let mut names = Vec::with_capacity(count as usize);
        let mut power_limits = Vec::with_capacity(count as usize);
        let mut temperature_limits = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut device = ptr::null_mut();
            if unsafe { get_handle(index, &mut device) } != NVML_SUCCESS {
                continue;
            }
            let mut name = [0 as c_char; 64];
            let clean_name =
                if unsafe { (functions.get_name)(device, name.as_mut_ptr(), name.len() as c_uint) }
                    == NVML_SUCCESS
                {
                    clean_nvidia_name(
                        unsafe { CStr::from_ptr(name.as_ptr()) }
                            .to_string_lossy()
                            .as_ref(),
                    )
                } else {
                    "NVIDIA GPU".to_string()
                };
            let mut power_limit = 255_000;
            let _ = unsafe { (functions.get_power_limit)(device, &mut power_limit) };
            let mut temperature_limit = 110;
            let _ =
                unsafe { (functions.get_temperature_threshold)(device, 0, &mut temperature_limit) };
            devices.push(device);
            names.push(clean_name);
            power_limits.push(u64::from(power_limit));
            temperature_limits.push(i64::from(temperature_limit));
        }
        if devices.is_empty() {
            unsafe { shutdown() };
            return None;
        }
        let mut backend = Self {
            _library: library,
            functions,
            devices,
            names,
            power_limits,
            temperature_limits,
            supports: Vec::new(),
        };
        backend.supports = backend
            .collect_unchecked(true, true)
            .into_iter()
            .map(|sample| sample.support)
            .collect();
        Some(backend)
    }

    fn collect(&self, check_temperature: bool, measure_pcie: bool) -> Vec<GpuSample> {
        let mut samples = self.collect_unchecked(check_temperature, measure_pcie);
        for (sample, support) in samples.iter_mut().zip(&self.supports) {
            sample.support.clone_from(support);
        }
        samples
    }

    fn collect_unchecked(&self, check_temperature: bool, measure_pcie: bool) -> Vec<GpuSample> {
        self.devices
            .iter()
            .enumerate()
            .map(|(index, &device)| {
                let mut sample = GpuSample {
                    name: self.names[index].clone(),
                    power_limit_mw: self.power_limits[index],
                    temperature_max_c: self.temperature_limits[index],
                    ..GpuSample::default()
                };
                let mut utilization = NvmlUtilization::default();
                if unsafe { (self.functions.get_utilization)(device, &mut utilization) }
                    == NVML_SUCCESS
                {
                    sample.utilization = utilization.gpu;
                    sample.memory_utilization = utilization.memory;
                    sample.support.utilization = true;
                    sample.support.memory_utilization = true;
                }
                sample.support.gpu_clock = nvml_uint_kind(
                    self.functions.get_clock,
                    device,
                    NVML_CLOCK_GRAPHICS,
                    &mut sample.gpu_clock_mhz,
                );
                sample.support.memory_clock = nvml_uint_kind(
                    self.functions.get_clock,
                    device,
                    NVML_CLOCK_MEMORY,
                    &mut sample.memory_clock_mhz,
                );
                let mut power = 0;
                if unsafe { (self.functions.get_power)(device, &mut power) } == NVML_SUCCESS {
                    sample.power_mw = u64::from(power);
                    sample.support.power = true;
                }
                let mut power_state = 32;
                if unsafe { (self.functions.get_power_state)(device, &mut power_state) }
                    == NVML_SUCCESS
                {
                    sample.power_state = power_state as i32;
                    sample.support.power_state = true;
                }
                if check_temperature {
                    let mut temperature = 0;
                    if unsafe { (self.functions.get_temperature)(device, 0, &mut temperature) }
                        == NVML_SUCCESS
                    {
                        sample.temperature_c = i64::from(temperature);
                        sample.support.temperature = true;
                    }
                } else {
                    sample.support.temperature = true;
                }
                let mut memory = NvmlMemory::default();
                if unsafe { (self.functions.get_memory)(device, &mut memory) } == NVML_SUCCESS {
                    sample.memory_total = memory.total;
                    sample.memory_used = memory.used;
                    sample.support.memory_total = true;
                    sample.support.memory_used = true;
                }
                if measure_pcie {
                    let function = self.functions.get_pcie;
                    let device_address = device as usize;
                    let (tx_result, rx_result) = std::thread::scope(|scope| {
                        let tx = scope.spawn(move || {
                            let mut value = 0;
                            let result =
                                unsafe { function(device_address as NvmlDevice, 0, &mut value) };
                            (result, value)
                        });
                        let rx = scope.spawn(move || {
                            let mut value = 0;
                            let result =
                                unsafe { function(device_address as NvmlDevice, 1, &mut value) };
                            (result, value)
                        });
                        (tx.join(), rx.join())
                    });
                    if let (Ok((NVML_SUCCESS, tx)), Ok((NVML_SUCCESS, rx))) = (tx_result, rx_result)
                    {
                        sample.pcie_tx_kib = i64::from(tx);
                        sample.pcie_rx_kib = i64::from(rx);
                        sample.support.pcie = true;
                    }
                }
                let mut period = 0;
                sample.support.encoder = nvml_codec(
                    self.functions.get_encoder,
                    device,
                    &mut sample.encoder_utilization,
                    &mut period,
                );
                sample.support.decoder = nvml_codec(
                    self.functions.get_decoder,
                    device,
                    &mut sample.decoder_utilization,
                    &mut period,
                );
                sample
            })
            .collect()
    }
}

impl Drop for Nvml {
    fn drop(&mut self) {
        unsafe { (self.functions.shutdown)() };
    }
}

fn nvml_uint_kind(
    function: NvmlDeviceGetKindUint,
    device: NvmlDevice,
    kind: c_int,
    target: &mut u32,
) -> bool {
    (unsafe { function(device, kind, target) }) == NVML_SUCCESS
}

fn nvml_codec(
    function: NvmlDeviceGetCodec,
    device: NvmlDevice,
    target: &mut u32,
    period: &mut u32,
) -> bool {
    (unsafe { function(device, target, period) }) == NVML_SUCCESS
}

fn clean_nvidia_name(name: &str) -> String {
    ["NVIDIA", "Nvidia", "(R)", "(TM)"]
        .into_iter()
        .fold(name.to_string(), |value, brand| value.replace(brand, ""))
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

const NVML_SUCCESS: c_int = 0;
const NVML_CLOCK_GRAPHICS: c_int = 0;
const NVML_CLOCK_MEMORY: c_int = 2;

#[repr(C)]
struct RsmiVersion {
    major: u32,
    minor: u32,
    patch: u32,
    build: *const c_char,
}

#[repr(C)]
#[derive(Default)]
struct RsmiFrequenciesV5 {
    count: u32,
    current: u32,
    frequencies: [u64; 32],
}

#[repr(C)]
struct RsmiFrequenciesV6 {
    has_deep_sleep: bool,
    count: u32,
    current: u32,
    frequencies: [u64; 33],
}

impl Default for RsmiFrequenciesV6 {
    fn default() -> Self {
        Self {
            has_deep_sleep: false,
            count: 0,
            current: 0,
            frequencies: [0; 33],
        }
    }
}

type RsmiStatus = c_int;
type RsmiInit = unsafe extern "C" fn(u64) -> RsmiStatus;
type RsmiShutdown = unsafe extern "C" fn() -> RsmiStatus;
type RsmiVersionGet = unsafe extern "C" fn(*mut RsmiVersion) -> RsmiStatus;
type RsmiCount = unsafe extern "C" fn(*mut u32) -> RsmiStatus;
type RsmiName = unsafe extern "C" fn(u32, *mut c_char, usize) -> RsmiStatus;
type RsmiU32 = unsafe extern "C" fn(u32, *mut u32) -> RsmiStatus;
type RsmiU64Kind = unsafe extern "C" fn(u32, u32, *mut u64) -> RsmiStatus;
type RsmiI64Kind = unsafe extern "C" fn(u32, u32, c_int, *mut i64) -> RsmiStatus;
type RsmiMemory = unsafe extern "C" fn(u32, c_int, *mut u64) -> RsmiStatus;
type RsmiPcie = unsafe extern "C" fn(u32, *mut u64, *mut u64, *mut u64) -> RsmiStatus;
type RsmiClockV5 = unsafe extern "C" fn(u32, c_int, *mut RsmiFrequenciesV5) -> RsmiStatus;
type RsmiClockV6 = unsafe extern "C" fn(u32, c_int, *mut RsmiFrequenciesV6) -> RsmiStatus;

enum RsmiClock {
    V5(RsmiClockV5),
    V6(RsmiClockV6),
}

struct RsmiFunctions {
    shutdown: RsmiShutdown,
    name: RsmiName,
    power_cap: RsmiU64Kind,
    temperature: RsmiI64Kind,
    busy: RsmiU32,
    memory_busy: RsmiU32,
    clock: RsmiClock,
    power: RsmiU64Kind,
    memory_total: RsmiMemory,
    memory_used: RsmiMemory,
    pcie: RsmiPcie,
}

struct Rsmi {
    _library: DynamicLibrary,
    functions: RsmiFunctions,
    count: u32,
    names: Vec<String>,
    power_limits: Vec<u64>,
    temperature_limits: Vec<i64>,
    supports: Vec<GpuSupport>,
}

impl Rsmi {
    fn load() -> Option<Self> {
        let library = DynamicLibrary::open(&[
            "/opt/rocm/lib/librocm_smi64.so",
            "librocm_smi64.so",
            "librocm_smi64.so.5",
            "librocm_smi64.so.1.0",
            "librocm_smi64.so.6",
            "librocm_smi64.so.7",
        ])?;
        let init: RsmiInit = library.symbol(b"rsmi_init\0")?;
        let shutdown: RsmiShutdown = library.symbol(b"rsmi_shut_down\0")?;
        let version_get: RsmiVersionGet = library.symbol(b"rsmi_version_get\0")?;
        let get_count: RsmiCount = library.symbol(b"rsmi_num_monitor_devices\0")?;
        if unsafe { init(0) } != RSMI_SUCCESS {
            return None;
        }
        let mut version = RsmiVersion {
            major: 0,
            minor: 0,
            patch: 0,
            build: ptr::null(),
        };
        if unsafe { version_get(&mut version) } != RSMI_SUCCESS {
            unsafe { shutdown() };
            return None;
        }
        let effective_major = if version.major == 1 {
            if library
                .symbol::<*mut c_void>(b"rsmi_dev_activity_metric_get\0")
                .is_some()
            {
                6
            } else {
                5
            }
        } else {
            version.major
        };
        let clock_pointer = unsafe { dlsym(library.handle, c"rsmi_dev_gpu_clk_freq_get".as_ptr()) };
        if clock_pointer.is_null() {
            unsafe { shutdown() };
            return None;
        }
        let clock = match effective_major {
            5 => {
                RsmiClock::V5(unsafe { mem::transmute::<*mut c_void, RsmiClockV5>(clock_pointer) })
            }
            6 | 7 => {
                RsmiClock::V6(unsafe { mem::transmute::<*mut c_void, RsmiClockV6>(clock_pointer) })
            }
            _ => {
                unsafe { shutdown() };
                return None;
            }
        };
        let functions = RsmiFunctions {
            shutdown,
            name: library.symbol(b"rsmi_dev_name_get\0")?,
            power_cap: library.symbol(b"rsmi_dev_power_cap_get\0")?,
            temperature: library.symbol(b"rsmi_dev_temp_metric_get\0")?,
            busy: library.symbol(b"rsmi_dev_busy_percent_get\0")?,
            memory_busy: library.symbol(b"rsmi_dev_memory_busy_percent_get\0")?,
            clock,
            power: library.symbol(b"rsmi_dev_power_ave_get\0")?,
            memory_total: library.symbol(b"rsmi_dev_memory_total_get\0")?,
            memory_used: library.symbol(b"rsmi_dev_memory_usage_get\0")?,
            pcie: library.symbol(b"rsmi_dev_pci_throughput_get\0")?,
        };
        let mut count = 0;
        if unsafe { get_count(&mut count) } != RSMI_SUCCESS || count == 0 {
            unsafe { shutdown() };
            return None;
        }
        let mut names = Vec::with_capacity(count as usize);
        let mut power_limits = Vec::with_capacity(count as usize);
        let mut temperature_limits = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut name = [0 as c_char; 128];
            names.push(
                if unsafe { (functions.name)(index, name.as_mut_ptr(), name.len()) } == RSMI_SUCCESS
                {
                    unsafe { CStr::from_ptr(name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned()
                } else {
                    "AMD GPU".to_string()
                },
            );
            let mut power = 225_000_000;
            let _ = unsafe { (functions.power_cap)(index, 0, &mut power) };
            power_limits.push(power / 1000);
            let mut temperature = 110_000;
            let _ = unsafe { (functions.temperature)(index, 0, 1, &mut temperature) };
            temperature_limits.push(temperature / 1000);
        }
        let mut backend = Self {
            _library: library,
            functions,
            count,
            names,
            power_limits,
            temperature_limits,
            supports: Vec::new(),
        };
        backend.supports = backend
            .collect_unchecked(true, true)
            .into_iter()
            .map(|sample| sample.support)
            .collect();
        Some(backend)
    }

    fn collect(&self, check_temperature: bool, measure_pcie: bool) -> Vec<GpuSample> {
        let mut samples = self.collect_unchecked(check_temperature, measure_pcie);
        for (sample, support) in samples.iter_mut().zip(&self.supports) {
            sample.support.clone_from(support);
        }
        samples
    }

    fn collect_unchecked(&self, check_temperature: bool, measure_pcie: bool) -> Vec<GpuSample> {
        (0..self.count)
            .map(|index| {
                let mut sample = GpuSample {
                    name: self.names[index as usize].clone(),
                    power_limit_mw: self.power_limits[index as usize],
                    temperature_max_c: self.temperature_limits[index as usize],
                    ..GpuSample::default()
                };
                sample.support.utilization =
                    rsmi_u32(self.functions.busy, index, &mut sample.utilization);
                sample.support.memory_utilization = rsmi_u32(
                    self.functions.memory_busy,
                    index,
                    &mut sample.memory_utilization,
                );
                sample.support.gpu_clock =
                    self.read_clock(index, RSMI_CLOCK_SYSTEM, &mut sample.gpu_clock_mhz);
                sample.support.memory_clock =
                    self.read_clock(index, RSMI_CLOCK_MEMORY, &mut sample.memory_clock_mhz);
                let mut power = 0;
                if unsafe { (self.functions.power)(index, 0, &mut power) } == RSMI_SUCCESS {
                    sample.power_mw = power / 1000;
                    sample.power_limit_mw = sample.power_limit_mw.max(sample.power_mw);
                    sample.support.power = true;
                }
                if check_temperature {
                    let mut temperature = 0;
                    if unsafe { (self.functions.temperature)(index, 0, 0, &mut temperature) }
                        == RSMI_SUCCESS
                    {
                        sample.temperature_c = temperature / 1000;
                        sample.support.temperature = true;
                    }
                } else {
                    sample.support.temperature = true;
                }
                sample.support.memory_total =
                    rsmi_memory(self.functions.memory_total, index, &mut sample.memory_total);
                sample.support.memory_used =
                    rsmi_memory(self.functions.memory_used, index, &mut sample.memory_used);
                if measure_pcie {
                    let mut tx = 0;
                    let mut rx = 0;
                    if unsafe { (self.functions.pcie)(index, &mut tx, &mut rx, ptr::null_mut()) }
                        == RSMI_SUCCESS
                    {
                        sample.pcie_tx_kib = tx as i64;
                        sample.pcie_rx_kib = rx as i64;
                        sample.support.pcie = true;
                    }
                }
                sample
            })
            .collect()
    }

    fn read_clock(&self, index: u32, kind: c_int, target: &mut u32) -> bool {
        let frequency = match self.functions.clock {
            RsmiClock::V5(function) => {
                let mut frequencies = RsmiFrequenciesV5::default();
                if unsafe { function(index, kind, &mut frequencies) } != RSMI_SUCCESS
                    || frequencies.count == 0
                    || frequencies.count > frequencies.frequencies.len() as u32
                    || frequencies.current >= frequencies.count
                {
                    return false;
                }
                frequencies.frequencies[frequencies.current as usize]
            }
            RsmiClock::V6(function) => {
                let mut frequencies = RsmiFrequenciesV6::default();
                if unsafe { function(index, kind, &mut frequencies) } != RSMI_SUCCESS
                    || frequencies.count == 0
                    || frequencies.count > frequencies.frequencies.len() as u32
                    || frequencies.current >= frequencies.count
                {
                    return false;
                }
                frequencies.frequencies[frequencies.current as usize]
            }
        };
        *target = (frequency / 1_000_000) as u32;
        true
    }
}

impl Drop for Rsmi {
    fn drop(&mut self) {
        unsafe { (self.functions.shutdown)() };
    }
}

fn rsmi_u32(function: RsmiU32, index: u32, target: &mut u32) -> bool {
    (unsafe { function(index, target) }) == RSMI_SUCCESS
}

fn rsmi_memory(function: RsmiMemory, index: u32, target: &mut u64) -> bool {
    (unsafe { function(index, RSMI_MEMORY_VRAM, target) }) == RSMI_SUCCESS
}

const RSMI_SUCCESS: c_int = 0;
const RSMI_MEMORY_VRAM: c_int = 0;
const RSMI_CLOCK_SYSTEM: c_int = 0;
const RSMI_CLOCK_MEMORY: c_int = 4;

struct AmdSysfsDevice {
    name: String,
    device: PathBuf,
    hwmon: Option<PathBuf>,
    power: Option<PathBuf>,
    power_max_mw: u64,
}

impl AmdSysfsDevice {
    fn collect(&mut self) -> GpuSample {
        let busy = read_integer(self.device.join("gpu_busy_percent"));
        let total = read_integer(self.device.join("mem_info_vram_total"));
        let used = read_integer(self.device.join("mem_info_vram_used"));
        let temperature = self
            .hwmon
            .as_ref()
            .and_then(|path| read_integer(path.join("temp1_input")));
        let clock = self
            .hwmon
            .as_ref()
            .and_then(|path| read_integer(path.join("freq1_input")));
        let power = self.power.as_ref().and_then(read_integer);
        let power_mw = power.unwrap_or(0).max(0) as u64 / 1000;
        self.power_max_mw = self.power_max_mw.max(power_mw);
        GpuSample {
            name: self.name.clone(),
            utilization: busy.unwrap_or(0).clamp(0, 100) as u32,
            gpu_clock_mhz: clock.unwrap_or(0).max(0) as u32 / 1_000_000,
            power_mw,
            power_limit_mw: self.power_max_mw,
            temperature_c: temperature.unwrap_or(0) / 1000,
            memory_total: total.unwrap_or(0).max(0) as u64,
            memory_used: used.unwrap_or(0).max(0) as u64,
            support: GpuSupport {
                utilization: busy.is_some(),
                gpu_clock: clock.is_some(),
                power: power.is_some(),
                temperature: temperature.is_some(),
                memory_total: total.is_some(),
                memory_used: used.is_some(),
                ..GpuSupport::default()
            },
            ..GpuSample::default()
        }
    }
}

fn discover_amd_sysfs() -> Vec<AmdSysfsDevice> {
    discover_amd_sysfs_at(Path::new("/sys/class/drm"))
}

fn discover_amd_sysfs_at(root: &Path) -> Vec<AmdSysfsDevice> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let card_name = entry.file_name().to_string_lossy().into_owned();
        if !is_card_node(&card_name) {
            continue;
        }
        let device = entry.path().join("device");
        if read_trimmed(device.join("vendor")).as_deref() != Some("0x1002") {
            continue;
        }
        let driver = fs::read_link(device.join("driver")).ok().and_then(|path| {
            path.file_name()
                .map(|name| name.to_string_lossy().into_owned())
        });
        if driver.as_deref() != Some("amdgpu") {
            continue;
        }
        let id = read_trimmed(device.join("device")).unwrap_or_else(|| "0x0000".into());
        let id = id.trim_start_matches("0x");
        let hwmon = fs::read_dir(device.join("hwmon"))
            .ok()
            .and_then(|mut entries| entries.find_map(Result::ok))
            .map(|entry| entry.path());
        let power = hwmon.as_ref().and_then(|path| {
            ["power1_average", "power1_input"]
                .into_iter()
                .map(|name| path.join(name))
                .find(|candidate| candidate.exists())
        });
        let has_signal = device.join("gpu_busy_percent").exists()
            || device.join("mem_info_vram_total").exists()
            || hwmon.as_ref().is_some_and(|path| {
                path.join("temp1_input").exists()
                    || path.join("freq1_input").exists()
                    || power.is_some()
            });
        if has_signal {
            devices.push(AmdSysfsDevice {
                name: format!("AMD GPU (1002:{id})"),
                device,
                hwmon,
                power,
                power_max_mw: 0,
            });
        }
    }
    devices
}

struct PmuCounter {
    fd: RawFd,
    previous: u64,
    scale: f64,
}

impl PmuCounter {
    fn open(pmu_type: u32, event: &Path, scale: f64) -> Option<Self> {
        let config = parse_perf_config(&fs::read_to_string(event).ok()?)?;
        let mut attributes = PerfEventAttr {
            event_type: pmu_type,
            size: mem::size_of::<PerfEventAttr>() as u32,
            config,
            ..PerfEventAttr::default()
        };
        let fd = unsafe {
            syscall(
                SYS_PERF_EVENT_OPEN,
                &mut attributes as *mut PerfEventAttr,
                -1 as c_int,
                0 as c_int,
                -1 as c_int,
                PERF_FLAG_FD_CLOEXEC,
            ) as RawFd
        };
        if fd < 0 {
            return None;
        }
        let mut counter = Self {
            fd,
            previous: 0,
            scale,
        };
        counter.previous = counter.read().unwrap_or(0);
        Some(counter)
    }

    fn read(&self) -> Option<u64> {
        let mut value = 0_u64;
        let count = unsafe {
            read(
                self.fd,
                (&mut value as *mut u64).cast(),
                mem::size_of::<u64>(),
            )
        };
        (count == mem::size_of::<u64>() as isize).then_some(value)
    }

    fn delta(&mut self) -> u64 {
        let current = self.read().unwrap_or(self.previous);
        let delta = current.saturating_sub(self.previous);
        self.previous = current;
        delta
    }
}

impl Drop for PmuCounter {
    fn drop(&mut self) {
        unsafe { close(self.fd) };
    }
}

struct IntelPmu {
    name: String,
    busy: Vec<PmuCounter>,
    frequency: Option<PmuCounter>,
    energy: Option<PmuCounter>,
    last_sample: Instant,
    max_power_mw: u64,
}

impl IntelPmu {
    fn load() -> Option<Self> {
        let root = Path::new("/sys/bus/event_source/devices/i915");
        let pmu_type = read_trimmed(root.join("type"))?.parse().ok()?;
        let events = root.join("events");
        let mut busy = Vec::new();
        for entry in fs::read_dir(&events).ok()?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.ends_with("-busy")
                && let Some(counter) = PmuCounter::open(pmu_type, &entry.path(), 1.0)
            {
                busy.push(counter);
            }
        }
        if busy.is_empty() {
            return None;
        }
        let frequency = PmuCounter::open(pmu_type, &events.join("actual-frequency"), 1.0);
        let energy_scale = read_trimmed(events.join("energy-gpu.scale"))
            .and_then(|value| value.parse().ok())
            .unwrap_or(1.0);
        let energy = PmuCounter::open(pmu_type, &events.join("energy-gpu"), energy_scale);
        Some(Self {
            name: discover_intel_name(),
            busy,
            frequency,
            energy,
            last_sample: Instant::now(),
            max_power_mw: 10_000,
        })
    }

    fn collect(&mut self) -> GpuSample {
        let elapsed = self.last_sample.elapsed().as_secs_f64().max(0.000_001);
        self.last_sample = Instant::now();
        let utilization = self
            .busy
            .iter_mut()
            .map(|counter| counter.delta() as f64 / 1e9 / elapsed * 100.0)
            .fold(0.0, f64::max)
            .round()
            .clamp(0.0, 100.0) as u32;
        let gpu_clock_mhz = self
            .frequency
            .as_mut()
            .map(|counter| (counter.delta() as f64 / 1e9 / elapsed).round() as u32)
            .unwrap_or(0);
        let power_mw = self
            .energy
            .as_mut()
            .map(|counter| (counter.delta() as f64 * counter.scale / elapsed * 1000.0) as u64)
            .unwrap_or(0);
        self.max_power_mw = self.max_power_mw.max(power_mw);
        GpuSample {
            name: self.name.clone(),
            utilization,
            gpu_clock_mhz,
            power_mw,
            power_limit_mw: self.max_power_mw,
            support: GpuSupport {
                utilization: true,
                gpu_clock: self.frequency.is_some(),
                power: self.energy.is_some(),
                ..GpuSupport::default()
            },
            ..GpuSample::default()
        }
    }
}

fn discover_intel_name() -> String {
    let Ok(entries) = fs::read_dir("/sys/class/drm") else {
        return "Intel GPU".into();
    };
    for entry in entries.flatten() {
        let card = entry.file_name().to_string_lossy().into_owned();
        if !is_card_node(&card) {
            continue;
        }
        let device = entry.path().join("device");
        if read_trimmed(device.join("vendor")).as_deref() == Some("0x8086") {
            let id = read_trimmed(device.join("device")).unwrap_or_else(|| "0x0000".into());
            if let Ok(device_id) = u16::from_str_radix(id.trim_start_matches("0x"), 16)
                && let Some(name) = intel_device_name(device_id)
            {
                return name;
            }
            return format!("Intel GPU (8086:{})", id.trim_start_matches("0x"));
        }
    }
    "Intel GPU".into()
}

fn intel_device_name(device_id: u16) -> Option<String> {
    const DATABASE: &str = include_str!("../assets/intel-gpu-names.txt");
    let needle = format!("{device_id:04x}");
    let (_, codename, generation) = DATABASE.lines().find_map(|line| {
        let mut fields = line.split('|');
        let id = fields.next()?;
        let codename = fields.next()?;
        let generation = fields.next()?;
        (id == needle).then_some((id, codename, generation))
    })?;
    let mut chars = codename.chars();
    let codename = chars
        .next()
        .map(|first| first.to_ascii_uppercase().to_string() + chars.as_str())?;
    Some(format!("Intel {codename} (Gen{generation})"))
}

#[repr(C)]
#[derive(Default)]
struct PerfEventAttr {
    event_type: u32,
    size: u32,
    config: u64,
    sample_period: u64,
    sample_type: u64,
    read_format: u64,
    flags: u64,
    wakeup_events: u32,
    bp_type: u32,
    config1: u64,
    config2: u64,
    branch_sample_type: u64,
    sample_regs_user: u64,
    sample_stack_user: u32,
    clockid: i32,
    sample_regs_intr: u64,
    aux_watermark: u32,
    sample_max_stack: u16,
    reserved: u16,
    aux_sample_size: u32,
    reserved_2: u32,
    sig_data: u64,
}

fn parse_perf_config(value: &str) -> Option<u64> {
    value.split(',').find_map(|field| {
        let (name, raw) = field.trim().split_once('=')?;
        (name == "event").then(|| u64::from_str_radix(raw.trim_start_matches("0x"), 16).ok())?
    })
}

fn is_card_node(name: &str) -> bool {
    name.strip_prefix("card").is_some_and(|suffix| {
        !suffix.is_empty() && suffix.bytes().all(|byte| byte.is_ascii_digit())
    })
}

fn read_trimmed(path: impl AsRef<Path>) -> Option<String> {
    Some(fs::read_to_string(path).ok()?.trim().to_string())
}

fn read_integer(path: impl AsRef<Path>) -> Option<i64> {
    read_trimmed(path)?.parse().ok()
}

const RTLD_LAZY: c_int = 1;
const PERF_FLAG_FD_CLOEXEC: c_ulong = 8;
#[cfg(target_arch = "x86_64")]
const SYS_PERF_EVENT_OPEN: c_long = 298;
#[cfg(target_arch = "aarch64")]
const SYS_PERF_EVENT_OPEN: c_long = 241;
#[cfg(target_arch = "x86")]
const SYS_PERF_EVENT_OPEN: c_long = 336;
#[cfg(target_arch = "arm")]
const SYS_PERF_EVENT_OPEN: c_long = 364;

#[link(name = "dl")]
unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> c_int;
    fn syscall(number: c_long, ...) -> c_long;
    fn read(fd: c_int, buffer: *mut c_void, count: usize) -> isize;
    fn close(fd: c_int) -> c_int;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn cleans_nvidia_brand_like_reference() {
        assert_eq!(clean_nvidia_name("NVIDIA RTX A4000"), "RTX A4000");
    }

    #[test]
    fn parses_i915_perf_event_config() {
        assert_eq!(parse_perf_config("event=0x1a"), Some(0x1a));
    }

    #[test]
    fn intel_names_match_the_complete_upstream_pci_database() {
        assert_eq!(
            intel_device_name(0x7121).as_deref(),
            Some("Intel Solano (Gen1)")
        );
        assert_eq!(
            intel_device_name(0x56a0).as_deref(),
            Some("Intel Dg2 (Gen12)")
        );
        assert_eq!(
            intel_device_name(0xe202).as_deref(),
            Some("Intel Battlemage (Gen20)")
        );
        assert_eq!(intel_device_name(0xffff), None);
    }

    #[test]
    fn rocm_frequency_structs_match_the_c_abi() {
        assert_eq!(mem::size_of::<RsmiFrequenciesV5>(), 264);
        assert_eq!(mem::size_of::<RsmiFrequenciesV6>(), 280);
    }

    #[test]
    fn discovers_and_collects_amdgpu_sysfs_fallback() {
        let root =
            std::env::temp_dir().join(format!("btop-rust-amdgpu-test-{}", std::process::id()));
        let device = root.join("card0/device");
        let hwmon = device.join("hwmon/hwmon0");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(device.join("vendor"), "0x1002\n").unwrap();
        fs::write(device.join("device"), "0x150e\n").unwrap();
        symlink("/sys/bus/pci/drivers/amdgpu", device.join("driver")).unwrap();
        fs::write(device.join("gpu_busy_percent"), "42\n").unwrap();
        fs::write(device.join("mem_info_vram_total"), "1073741824\n").unwrap();
        fs::write(device.join("mem_info_vram_used"), "268435456\n").unwrap();
        fs::write(hwmon.join("temp1_input"), "51000\n").unwrap();
        fs::write(hwmon.join("freq1_input"), "1800000000\n").unwrap();
        fs::write(hwmon.join("power1_average"), "35000000\n").unwrap();

        let mut devices = discover_amd_sysfs_at(&root);
        assert_eq!(devices.len(), 1);
        let sample = devices[0].collect();
        assert_eq!(sample.name, "AMD GPU (1002:150e)");
        assert_eq!(sample.utilization, 42);
        assert_eq!(sample.temperature_c, 51);
        assert_eq!(sample.gpu_clock_mhz, 1800);
        assert_eq!(sample.power_mw, 35_000);
        assert_eq!(sample.memory_used, 268_435_456);
        assert!(sample.support.memory_total);

        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    #[ignore = "requires a host NVIDIA GPU and access to NVML devices"]
    fn collects_live_nvml_device() {
        let nvml = Nvml::load().expect("NVML did not initialize");
        let samples = nvml.collect(true, true);
        assert!(!samples.is_empty());
        for sample in samples {
            eprintln!("{sample:#?}");
            assert!(!sample.name.is_empty());
            assert!(sample.support.utilization);
            assert!(sample.support.memory_total);
            assert!(sample.memory_total > 0);
        }
    }
}
