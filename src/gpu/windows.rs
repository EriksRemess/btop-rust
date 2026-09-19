use crate::config::Config;
use crate::logger;
use std::ffi::OsString;
use std::ffi::{CStr, c_char, c_int, c_uint, c_void};
use std::mem;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::PathBuf;
use std::ptr;
use std::time::{Duration, Instant};

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

pub struct GpuCollector {
    nvml: Option<Nvml>,
    shown: String,
    next_probe: Instant,
}

impl GpuCollector {
    pub fn new(config: &Config) -> Self {
        let shown = config.value("shown_gpus").unwrap_or("nvidia").to_string();
        let nvml = shown.contains("nvidia").then(Nvml::load).flatten();
        Self {
            nvml,
            shown,
            next_probe: Instant::now() + Duration::from_secs(10),
        }
    }

    pub fn collect(&mut self, config: &Config) -> Vec<GpuSample> {
        let shown = config.value("shown_gpus").unwrap_or("nvidia");
        if shown != self.shown {
            *self = Self::new(config);
        } else if shown.contains("nvidia")
            && self.nvml.is_none()
            && Instant::now() >= self.next_probe
        {
            self.nvml = Nvml::load();
            self.next_probe = Instant::now() + Duration::from_secs(10);
        }
        let check_temperature = config.check_temperature;
        let measure_pcie = config
            .bool_value("nvml_measure_pcie_speeds")
            .unwrap_or(true);
        self.nvml
            .as_ref()
            .map(|nvml| nvml.collect(check_temperature, measure_pcie))
            .unwrap_or_default()
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
type NvmlSystemGetDriverVersion = unsafe extern "C" fn(*mut c_char, c_uint) -> NvmlReturn;

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
    driver_label: Option<String>,
    power_limits: Vec<u64>,
    temperature_limits: Vec<i64>,
    supports: Vec<GpuSupport>,
}

impl Nvml {
    fn load() -> Option<Self> {
        let library = DynamicLibrary::open()?;
        let init: NvmlInit = library
            .symbol(b"nvmlInit_v2\0")
            .or_else(|| library.symbol(b"nvmlInit\0"))?;
        let shutdown: NvmlShutdown = library.symbol(b"nvmlShutdown\0")?;
        let get_count: NvmlDeviceGetCount = library
            .symbol(b"nvmlDeviceGetCount_v2\0")
            .or_else(|| library.symbol(b"nvmlDeviceGetCount\0"))?;
        let get_handle: NvmlDeviceGetHandle = library
            .symbol(b"nvmlDeviceGetHandleByIndex_v2\0")
            .or_else(|| library.symbol(b"nvmlDeviceGetHandleByIndex\0"))?;
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
            logger::debug("Windows NVML initialization failed");
            return None;
        }
        let mut count = 0;
        if unsafe { get_count(&mut count) } != NVML_SUCCESS || count == 0 {
            unsafe { shutdown() };
            logger::debug("Windows NVML reported no devices");
            return None;
        }
        let driver_label = library
            .symbol::<NvmlSystemGetDriverVersion>(b"nvmlSystemGetDriverVersion\0")
            .and_then(|get_version| {
                let mut version = [0 as c_char; 96];
                (unsafe { get_version(version.as_mut_ptr(), version.len() as c_uint) }
                    == NVML_SUCCESS)
                    .then(|| {
                        format!(
                            "NVIDIA {}",
                            unsafe { CStr::from_ptr(version.as_ptr()) }.to_string_lossy()
                        )
                    })
            });
        let mut devices = Vec::with_capacity(count as usize);
        let mut names = Vec::with_capacity(count as usize);
        let mut power_limits = Vec::with_capacity(count as usize);
        let mut temperature_limits = Vec::with_capacity(count as usize);
        for index in 0..count {
            let mut device = ptr::null_mut();
            if unsafe { get_handle(index, &mut device) } != NVML_SUCCESS {
                continue;
            }
            let mut name = [0 as c_char; 96];
            let name =
                if unsafe { (functions.get_name)(device, name.as_mut_ptr(), name.len() as c_uint) }
                    == NVML_SUCCESS
                {
                    clean_nvidia_name(
                        unsafe { CStr::from_ptr(name.as_ptr()) }
                            .to_string_lossy()
                            .as_ref(),
                    )
                } else {
                    "NVIDIA GPU".into()
                };
            let mut power_limit = 255_000;
            let _ = unsafe { (functions.get_power_limit)(device, &mut power_limit) };
            let mut temperature_limit = 110;
            let _ =
                unsafe { (functions.get_temperature_threshold)(device, 0, &mut temperature_limit) };
            devices.push(device);
            names.push(name);
            power_limits.push(u64::from(power_limit));
            temperature_limits.push(i64::from(temperature_limit));
        }
        if devices.is_empty() {
            unsafe { shutdown() };
            return None;
        }
        let mut nvml = Self {
            _library: library,
            functions,
            devices,
            names,
            driver_label,
            power_limits,
            temperature_limits,
            supports: Vec::new(),
        };
        nvml.supports = nvml
            .collect_unchecked(true, true)
            .into_iter()
            .map(|sample| sample.support)
            .collect();
        Some(nvml)
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
                    driver_label: self.driver_label.clone(),
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
                    let mut transmit = 0;
                    let mut receive = 0;
                    if unsafe { (self.functions.get_pcie)(device, 0, &mut transmit) }
                        == NVML_SUCCESS
                        && unsafe { (self.functions.get_pcie)(device, 1, &mut receive) }
                            == NVML_SUCCESS
                    {
                        sample.pcie_tx_kib = i64::from(transmit);
                        sample.pcie_rx_kib = i64::from(receive);
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

struct DynamicLibrary(isize);

impl DynamicLibrary {
    fn open() -> Option<Self> {
        let mut candidates = Vec::new();
        if let Some(system) = system_directory() {
            candidates.push(system.join("nvml.dll"));
        }
        for variable in ["ProgramW6432", "ProgramFiles"] {
            if let Some(root) = std::env::var_os(variable) {
                let root = PathBuf::from(root);
                if root.is_absolute() {
                    candidates.push(root.join("NVIDIA Corporation/NVSMI/nvml.dll"));
                }
            }
        }
        for path in candidates {
            let wide: Vec<u16> = path
                .as_os_str()
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();
            let handle = unsafe {
                LoadLibraryExW(
                    wide.as_ptr(),
                    0,
                    LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR | LOAD_LIBRARY_SEARCH_SYSTEM32,
                )
            };
            if handle != 0 {
                return Some(Self(handle));
            }
        }
        logger::debug("Could not load nvml.dll on Windows");
        None
    }

    fn symbol<T: Copy>(&self, name: &'static [u8]) -> Option<T> {
        if mem::size_of::<T>() != mem::size_of::<*mut c_void>() {
            return None;
        }
        let pointer = unsafe { GetProcAddress(self.0, name.as_ptr()) };
        if pointer.is_null() {
            None
        } else {
            Some(unsafe { mem::transmute_copy(&pointer) })
        }
    }
}

fn system_directory() -> Option<PathBuf> {
    let mut buffer = vec![0u16; 32_768];
    let length = unsafe { GetSystemDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) } as usize;
    if length == 0 || length >= buffer.len() {
        return None;
    }
    Some(PathBuf::from(OsString::from_wide(&buffer[..length])))
}

impl Drop for DynamicLibrary {
    fn drop(&mut self) {
        if self.0 != 0 {
            unsafe { FreeLibrary(self.0) };
        }
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
const LOAD_LIBRARY_SEARCH_DLL_LOAD_DIR: u32 = 0x0000_0100;
const LOAD_LIBRARY_SEARCH_SYSTEM32: u32 = 0x0000_0800;

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn LoadLibraryExW(name: *const u16, file: isize, flags: u32) -> isize;
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    fn GetProcAddress(module: isize, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: isize) -> i32;
}

pub fn diagnostics() -> String {
    let Some(nvml) = Nvml::load() else {
        return "\nGPU\n  NVIDIA NVML: unavailable (nvml.dll not found or initialization failed)\n  AMD/Intel telemetry: not implemented yet\n".into();
    };
    let samples = nvml.collect(true, true);
    let names = samples
        .iter()
        .map(|sample| sample.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "\nGPU\n  NVIDIA NVML: available ({} device{})\n  devices: {names}\n  AMD/Intel telemetry: not implemented yet\n",
        samples.len(),
        if samples.len() == 1 { "" } else { "s" }
    )
}
