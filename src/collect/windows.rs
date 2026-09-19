use super::*;
use std::ffi::c_void;
use std::mem::{MaybeUninit, size_of};
use std::time::Duration;

type Handle = isize;
const INVALID_HANDLE_VALUE: Handle = -1;
const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: u32 = 8;
const SYSTEM_PAGEFILE_INFORMATION: u32 = 18;
const TH32CS_SNAPPROCESS: u32 = 0x0000_0002;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const PROCESS_VM_READ: u32 = 0x0010;
const TOKEN_QUERY: u32 = 0x0008;
const TOKEN_USER_CLASS: u32 = 1;
const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
const IF_OPER_STATUS_UP: u32 = 1;
const FILE_SHARE_READ: u32 = 0x0000_0001;
const FILE_SHARE_WRITE: u32 = 0x0000_0002;
const FILE_SHARE_DELETE: u32 = 0x0000_0004;
const OPEN_EXISTING: u32 = 3;
const IOCTL_DISK_PERFORMANCE: u32 = 0x0007_0020;
const DRIVE_FIXED: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct FileTime {
    low: u32,
    high: u32,
}

impl FileTime {
    fn ticks(self) -> u64 {
        (u64::from(self.high) << 32) | u64::from(self.low)
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ProcessorPerformanceInfo {
    idle_time: i64,
    kernel_time: i64,
    user_time: i64,
    dpc_time: i64,
    interrupt_time: i64,
    interrupt_count: u32,
}

#[repr(C)]
struct MemoryStatusEx {
    length: u32,
    memory_load: u32,
    total_physical: u64,
    available_physical: u64,
    total_page_file: u64,
    available_page_file: u64,
    total_virtual: u64,
    available_virtual: u64,
    available_extended_virtual: u64,
}

#[repr(C)]
struct PerformanceInfo {
    size: u32,
    commit_total: usize,
    commit_limit: usize,
    commit_peak: usize,
    physical_total: usize,
    physical_available: usize,
    system_cache: usize,
    kernel_total: usize,
    kernel_paged: usize,
    kernel_nonpaged: usize,
    page_size: usize,
    handle_count: u32,
    process_count: u32,
    thread_count: u32,
}

#[derive(Debug, Clone, Copy, Default)]
struct WindowsMemoryComposition {
    in_use: u64,
    modified: u64,
    standby: u64,
    free: u64,
}

#[repr(C)]
struct SystemPowerStatus {
    ac_line_status: u8,
    battery_flag: u8,
    battery_life_percent: u8,
    system_status_flag: u8,
    battery_life_time: u32,
    battery_full_life_time: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessEntry32W {
    size: u32,
    usage: u32,
    process_id: u32,
    default_heap_id: usize,
    module_id: u32,
    threads: u32,
    parent_process_id: u32,
    priority_class_base: i32,
    flags: u32,
    exe_file: [u16; 260],
}

impl Default for ProcessEntry32W {
    fn default() -> Self {
        Self {
            size: size_of::<Self>() as u32,
            usage: 0,
            process_id: 0,
            default_heap_id: 0,
            module_id: 0,
            threads: 0,
            parent_process_id: 0,
            priority_class_base: 0,
            flags: 0,
            exe_file: [0; 260],
        }
    }
}

#[repr(C)]
struct ProcessMemoryCounters {
    size: u32,
    page_fault_count: u32,
    peak_working_set_size: usize,
    working_set_size: usize,
    quota_peak_paged_pool_usage: usize,
    quota_paged_pool_usage: usize,
    quota_peak_non_paged_pool_usage: usize,
    quota_non_paged_pool_usage: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
}

#[repr(C)]
#[derive(Default)]
struct IoCounters {
    read_operation_count: u64,
    write_operation_count: u64,
    other_operation_count: u64,
    read_transfer_count: u64,
    write_transfer_count: u64,
    other_transfer_count: u64,
}

#[repr(C)]
struct UnicodeString {
    length: u16,
    maximum_length: u16,
    buffer: *const u16,
}

#[repr(C)]
struct SidAndAttributes {
    sid: *mut c_void,
    attributes: u32,
}

#[repr(C)]
struct TokenUser {
    user: SidAndAttributes,
}

#[repr(C)]
#[derive(Default)]
struct DiskPerformance {
    bytes_read: i64,
    bytes_written: i64,
    read_time: i64,
    write_time: i64,
    idle_time: i64,
    read_count: u32,
    write_count: u32,
    queue_depth: u32,
    split_count: u32,
    query_time: i64,
    storage_device_number: u32,
    storage_manager_name: [u16; 8],
}

#[repr(C)]
struct MibIfRow2 {
    interface_luid: u64,
    interface_index: u32,
    interface_guid: [u8; 16],
    alias: [u16; 257],
    description: [u16; 257],
    physical_address_length: u32,
    physical_address: [u8; 32],
    permanent_physical_address: [u8; 32],
    mtu: u32,
    interface_type: u32,
    tunnel_type: u32,
    media_type: u32,
    physical_medium_type: u32,
    access_type: u32,
    direction_type: u32,
    interface_and_oper_status_flags: u8,
    _status_padding: [u8; 3],
    operational_status: u32,
    admin_status: u32,
    media_connect_state: u32,
    network_guid: [u8; 16],
    connection_type: u32,
    transmit_link_speed: u64,
    receive_link_speed: u64,
    in_octets: u64,
    in_unicast_packets: u64,
    in_non_unicast_packets: u64,
    in_discards: u64,
    in_errors: u64,
    in_unknown_protocols: u64,
    in_unicast_octets: u64,
    in_multicast_octets: u64,
    in_broadcast_octets: u64,
    out_octets: u64,
    out_unicast_packets: u64,
    out_non_unicast_packets: u64,
    out_discards: u64,
    out_errors: u64,
    out_unicast_octets: u64,
    out_multicast_octets: u64,
    out_broadcast_octets: u64,
    out_queue_length: u64,
}

#[repr(C)]
struct MibIfTable2 {
    number_of_entries: u32,
    table: [MibIfRow2; 1],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct MibIpAddrRow {
    address: u32,
    interface_index: u32,
    mask: u32,
    broadcast_address: u32,
    reassembly_size: u32,
    unused: u16,
    address_type: u16,
}

#[repr(C)]
#[derive(Clone, Copy)]
union PdhFormattedValueUnion {
    double_value: f64,
    large_value: i64,
    pointer_value: *const c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PdhFormattedCounterValue {
    status: u32,
    value: PdhFormattedValueUnion,
}

#[link(name = "Kernel32")]
unsafe extern "system" {
    fn GlobalMemoryStatusEx(status: *mut MemoryStatusEx) -> i32;
    fn GetTickCount64() -> u64;
    fn GetSystemTimeAsFileTime(time: *mut FileTime);
    fn GetSystemPowerStatus(status: *mut SystemPowerStatus) -> i32;
    fn GetLogicalDriveStringsW(length: u32, buffer: *mut u16) -> u32;
    fn GetDiskFreeSpaceExW(
        directory: *const u16,
        available: *mut u64,
        total: *mut u64,
        free: *mut u64,
    ) -> i32;
    fn GetDriveTypeW(root_path: *const u16) -> u32;
    fn CreateFileW(
        file_name: *const u16,
        desired_access: u32,
        share_mode: u32,
        security_attributes: *mut c_void,
        creation_disposition: u32,
        flags_and_attributes: u32,
        template_file: Handle,
    ) -> Handle;
    fn DeviceIoControl(
        device: Handle,
        control_code: u32,
        input: *mut c_void,
        input_size: u32,
        output: *mut c_void,
        output_size: u32,
        bytes_returned: *mut u32,
        overlapped: *mut c_void,
    ) -> i32;
    fn CreateToolhelp32Snapshot(flags: u32, process_id: u32) -> Handle;
    fn Process32FirstW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn Process32NextW(snapshot: Handle, entry: *mut ProcessEntry32W) -> i32;
    fn OpenProcess(access: u32, inherit: i32, process_id: u32) -> Handle;
    fn CloseHandle(handle: Handle) -> i32;
    fn GetProcessTimes(
        process: Handle,
        creation: *mut FileTime,
        exit: *mut FileTime,
        kernel: *mut FileTime,
        user: *mut FileTime,
    ) -> i32;
    fn GetProcessIoCounters(process: Handle, counters: *mut IoCounters) -> i32;
    fn GetPriorityClass(process: Handle) -> u32;
    fn QueryFullProcessImageNameW(
        process: Handle,
        flags: u32,
        path: *mut u16,
        length: *mut u32,
    ) -> i32;
    fn K32GetProcessMemoryInfo(
        process: Handle,
        counters: *mut ProcessMemoryCounters,
        size: u32,
    ) -> i32;
    fn K32GetPerformanceInfo(info: *mut PerformanceInfo, size: u32) -> i32;
}

#[link(name = "Pdh")]
unsafe extern "system" {
    fn PdhOpenQueryW(data_source: *const u16, user_data: usize, query: *mut Handle) -> i32;
    fn PdhAddEnglishCounterW(
        query: Handle,
        path: *const u16,
        user_data: usize,
        counter: *mut Handle,
    ) -> i32;
    fn PdhCollectQueryData(query: Handle) -> i32;
    fn PdhGetFormattedCounterValue(
        counter: Handle,
        format: u32,
        value_type: *mut u32,
        value: *mut PdhFormattedCounterValue,
    ) -> i32;
    fn PdhCloseQuery(query: Handle) -> i32;
}

#[link(name = "Ntdll")]
unsafe extern "system" {
    fn NtQuerySystemInformation(
        class: u32,
        information: *mut c_void,
        length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn NtQueryInformationProcess(
        process: Handle,
        information_class: u32,
        information: *mut c_void,
        length: u32,
        return_length: *mut u32,
    ) -> i32;
}

#[link(name = "Iphlpapi")]
unsafe extern "system" {
    fn GetIfTable2(table: *mut *mut MibIfTable2) -> u32;
    fn FreeMibTable(memory: *mut c_void);
    fn GetBestInterface(destination: u32, interface_index: *mut u32) -> u32;
    fn GetIpAddrTable(table: *mut c_void, size: *mut u32, order: i32) -> u32;
}

#[link(name = "Advapi32")]
unsafe extern "system" {
    fn RegGetValueW(
        key: isize,
        subkey: *const u16,
        value: *const u16,
        flags: u32,
        value_type: *mut u32,
        data: *mut c_void,
        data_size: *mut u32,
    ) -> i32;
    fn OpenProcessToken(process: Handle, access: u32, token: *mut Handle) -> i32;
    fn GetTokenInformation(
        token: Handle,
        information_class: u32,
        information: *mut c_void,
        information_length: u32,
        return_length: *mut u32,
    ) -> i32;
    fn GetLengthSid(sid: *const c_void) -> u32;
    fn LookupAccountSidW(
        system_name: *const u16,
        sid: *const c_void,
        name: *mut u16,
        name_length: *mut u32,
        domain: *mut u16,
        domain_length: *mut u32,
        sid_name_use: *mut u32,
    ) -> i32;
}

struct OwnedHandle(Handle);

impl OwnedHandle {
    fn new(handle: Handle) -> Option<Self> {
        (handle != 0 && handle != INVALID_HANDLE_VALUE).then_some(Self(handle))
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        unsafe { CloseHandle(self.0) };
    }
}

pub(super) fn read_cpu_name() -> String {
    registry_cpu_name()
        .or_else(|| std::env::var("PROCESSOR_IDENTIFIER").ok())
        .map(|name| super::clean_cpu_name(name.trim()))
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "Windows CPU".into())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

fn registry_cpu_name() -> Option<String> {
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as isize;
    const RRF_RT_REG_SZ: u32 = 0x0000_0002;
    let subkey = wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = wide("ProcessorNameString");
    let mut buffer = [0u16; 256];
    let mut bytes = std::mem::size_of_val(&buffer) as u32;
    if unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_SZ,
            ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut bytes,
        )
    } != 0
    {
        return None;
    }
    let length = buffer
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(buffer.len());
    Some(
        String::from_utf16_lossy(&buffer[..length])
            .trim()
            .to_string(),
    )
    .filter(|name| !name.is_empty())
}

fn registry_cpu_frequency_mhz() -> Option<u32> {
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as isize;
    const RRF_RT_REG_DWORD: u32 = 0x0000_0010;
    let subkey = wide(r"HARDWARE\DESCRIPTION\System\CentralProcessor\0");
    let value = wide("~MHz");
    let mut mhz = 0u32;
    let mut bytes = size_of::<u32>() as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_LOCAL_MACHINE,
            subkey.as_ptr(),
            value.as_ptr(),
            RRF_RT_REG_DWORD,
            ptr::null_mut(),
            (&mut mhz as *mut u32).cast(),
            &mut bytes,
        )
    };
    (result == 0 && mhz > 0).then_some(mhz)
}

fn registry_cpu_frequency() -> String {
    registry_cpu_frequency_mhz()
        .map(|mhz| calculate_frequency("first", &[f64::from(mhz)]))
        .unwrap_or_default()
}

pub(super) struct CpuFrequencyCollector {
    query: Handle,
    counter: Handle,
    nominal_mhz: Option<u32>,
    next_sample: Instant,
    last_frequency: Option<String>,
}

impl CpuFrequencyCollector {
    pub(super) fn new() -> Self {
        const COUNTER_PATH: &str = r"\Processor Information(_Total)\% Processor Performance";
        let nominal_mhz = registry_cpu_frequency_mhz();
        let mut query = 0;
        let mut counter = 0;
        let path = wide(COUNTER_PATH);
        if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut query) } != 0
            || unsafe { PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) } != 0
        {
            if query != 0 {
                unsafe { PdhCloseQuery(query) };
            }
            return Self {
                query: 0,
                counter: 0,
                nominal_mhz,
                next_sample: Instant::now(),
                last_frequency: None,
            };
        }
        // Rate-style PDH counters need an initial sample before values become
        // available on the first scheduled collection.
        let _ = unsafe { PdhCollectQueryData(query) };
        Self {
            query,
            counter,
            nominal_mhz,
            next_sample: Instant::now() + Duration::from_secs(1),
            last_frequency: None,
        }
    }

    fn collect(&mut self) -> Option<String> {
        const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
        const PDH_FMT_NOCAP100: u32 = 0x0000_8000;
        const PDH_CSTATUS_VALID_DATA: u32 = 0;
        const PDH_CSTATUS_NEW_DATA: u32 = 1;
        let now = Instant::now();
        if self.query == 0 || self.counter == 0 || now < self.next_sample {
            return self.last_frequency.clone();
        }
        self.next_sample = now + Duration::from_secs(1);
        if unsafe { PdhCollectQueryData(self.query) } != 0 {
            return self.last_frequency.clone();
        }
        let mut value_type = 0;
        let mut value = PdhFormattedCounterValue {
            status: u32::MAX,
            value: PdhFormattedValueUnion { double_value: 0.0 },
        };
        if unsafe {
            PdhGetFormattedCounterValue(
                self.counter,
                PDH_FMT_DOUBLE | PDH_FMT_NOCAP100,
                &mut value_type,
                &mut value,
            )
        } != 0
            || !matches!(value.status, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA)
        {
            return self.last_frequency.clone();
        }
        let performance = unsafe { value.value.double_value };
        if !performance.is_finite() || performance < 0.0 {
            return self.last_frequency.clone();
        }
        let nominal = f64::from(self.nominal_mhz?);
        let frequency = calculate_frequency("first", &[nominal * performance / 100.0]);
        self.last_frequency = Some(frequency.clone());
        Some(frequency)
    }
}

impl Drop for CpuFrequencyCollector {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

pub(super) fn collect_cpu(
    collector: &mut Collector,
    config: &Config,
) -> Result<(CpuSample, u64), String> {
    let mut needed = 0u32;
    let probe = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
            ptr::null_mut(),
            0,
            &mut needed,
        )
    };
    if needed == 0 || (probe >= 0 && needed == 0) {
        return Err("Windows did not report processor performance data".into());
    }
    let count = needed as usize / size_of::<ProcessorPerformanceInfo>();
    let mut raw = vec![ProcessorPerformanceInfo::default(); count];
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
            raw.as_mut_ptr().cast(),
            needed,
            &mut needed,
        )
    };
    if status < 0 {
        return Err(format!(
            "NtQuerySystemInformation failed with NTSTATUS {status:#x}"
        ));
    }
    let current_cores: Vec<CpuTicks> = raw
        .iter()
        .map(|item| {
            let idle = item.idle_time.max(0) as u64;
            let kernel = item.kernel_time.max(0) as u64;
            let user = item.user_time.max(0) as u64;
            let total = kernel.saturating_add(user);
            let mut fields = [0; 10];
            fields[0] = user;
            fields[2] = kernel.saturating_sub(idle);
            fields[3] = idle;
            fields[5] = item.interrupt_time.max(0) as u64;
            CpuTicks {
                busy: total.saturating_sub(idle),
                total,
                fields,
            }
        })
        .collect();
    let aggregate = current_cores
        .iter()
        .fold(CpuTicks::default(), |mut sum, item| {
            sum.busy = sum.busy.saturating_add(item.busy);
            sum.total = sum.total.saturating_add(item.total);
            for (target, value) in sum.fields.iter_mut().zip(item.fields) {
                *target = target.saturating_add(value);
            }
            sum
        });
    let mut current = Vec::with_capacity(current_cores.len() + 1);
    current.push(aggregate);
    current.extend(current_cores);
    let percentages: Vec<f64> = current
        .iter()
        .enumerate()
        .map(|(index, now)| {
            let old = collector.previous_cpu.get(index).copied().unwrap_or(*now);
            let total = now.total.saturating_sub(old.total);
            let busy = now.busy.saturating_sub(old.busy);
            if total == 0 {
                0.0
            } else {
                busy as f64 * 100.0 / total as f64
            }
        })
        .collect();
    let old_total = collector.previous_cpu.first().copied().unwrap_or(aggregate);
    let total_delta = aggregate.total.saturating_sub(old_total.total);
    let old_fields = old_total.fields;
    let fields = [("user", 0usize), ("system", 2), ("idle", 3), ("irq", 5)]
        .into_iter()
        .map(|(name, index)| {
            let value = if total_delta == 0 {
                0.0
            } else {
                (aggregate.fields[index].saturating_sub(old_fields[index]) as f64 * 100.0
                    / total_delta as f64)
                    .clamp(0.0, 100.0)
            };
            (name.to_string(), value)
        })
        .collect();
    collector.previous_cpu = current;
    let (frequency, core_frequencies_mhz) = if config.show_cpu_frequency {
        (
            collector
                .windows_cpu_frequency
                .collect()
                .unwrap_or_else(registry_cpu_frequency),
            Vec::new(),
        )
    } else {
        (String::new(), Vec::new())
    };
    let battery = config
        .bool_value("show_battery")
        .unwrap_or(true)
        .then(read_battery)
        .flatten();
    Ok((
        CpuSample {
            total: percentages.first().copied().unwrap_or(0.0),
            fields,
            cores: percentages.into_iter().skip(1).collect(),
            // Windows has no native Unix load-average metric. NaN tells the
            // renderer to omit it instead of displaying invented zeroes.
            load: [f64::NAN; 3],
            frequency,
            core_frequencies_mhz,
            temperature: None,
            temperature_max: 100.0,
            core_temperatures: vec![None; count],
            name: collector.cpu_name.clone(),
            uptime: unsafe { GetTickCount64() } / 1_000,
            battery,
            watts: None,
            container_engine: None,
            active_cpus: None,
            available_batteries: Vec::new(),
        },
        total_delta,
    ))
}

fn read_battery() -> Option<BatterySample> {
    let mut status = MaybeUninit::<SystemPowerStatus>::uninit();
    if unsafe { GetSystemPowerStatus(status.as_mut_ptr()) } == 0 {
        return None;
    }
    let status = unsafe { status.assume_init() };
    (status.battery_flag != 128 && status.battery_life_percent != 255).then(|| BatterySample {
        percent: status.battery_life_percent,
        status: match status.ac_line_status {
            1 if status.battery_flag & 8 != 0 => "Charging",
            1 => "Full",
            _ => "Discharging",
        }
        .into(),
        watts: None,
        seconds: (status.battery_life_time != u32::MAX)
            .then_some(u64::from(status.battery_life_time)),
    })
}

pub(super) fn collect_memory(
    config: &Config,
    previous_disks: &mut HashMap<String, DiskCounters>,
    elapsed: f64,
) -> Result<MemorySample, String> {
    let mut status = MemoryStatusEx {
        length: size_of::<MemoryStatusEx>() as u32,
        memory_load: 0,
        total_physical: 0,
        available_physical: 0,
        total_page_file: 0,
        available_page_file: 0,
        total_virtual: 0,
        available_virtual: 0,
        available_extended_virtual: 0,
    };
    if unsafe { GlobalMemoryStatusEx(&mut status) } == 0 {
        return Err(format!(
            "GlobalMemoryStatusEx failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let disks = if config.show_disks {
        collect_disks(config, previous_disks, elapsed)
    } else {
        previous_disks.clear();
        Vec::new()
    };
    let fallback_swap_total = status.total_page_file.saturating_sub(status.total_physical);
    let fallback_swap_available = status
        .available_page_file
        .saturating_sub(status.available_physical);
    let mut performance = PerformanceInfo {
        size: size_of::<PerformanceInfo>() as u32,
        commit_total: 0,
        commit_limit: 0,
        commit_peak: 0,
        physical_total: 0,
        physical_available: 0,
        system_cache: 0,
        kernel_total: 0,
        kernel_paged: 0,
        kernel_nonpaged: 0,
        page_size: 0,
        handle_count: 0,
        process_count: 0,
        thread_count: 0,
    };
    let _ = unsafe { K32GetPerformanceInfo(&mut performance, size_of::<PerformanceInfo>() as u32) };
    let (swap_total, swap_used) = read_pagefile_usage(performance.page_size as u64).unwrap_or((
        fallback_swap_total,
        fallback_swap_total.saturating_sub(fallback_swap_available),
    ));
    let composition = read_memory_composition(status.total_physical, status.available_physical);
    Ok(MemorySample {
        total: status.total_physical,
        used: composition.in_use,
        modified: composition.modified,
        free: composition.free,
        available: composition.standby.saturating_add(composition.free),
        // On Windows this field carries the standby list so the shared sample
        // remains compact; the renderer labels it explicitly as Standby.
        cached: composition.standby,
        swap_total,
        swap_used,
        disks,
    })
}

fn read_pagefile_usage(page_size: u64) -> Option<(u64, u64)> {
    if page_size == 0 {
        return None;
    }
    let mut required = 0;
    let _ = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PAGEFILE_INFORMATION,
            ptr::null_mut(),
            0,
            &mut required,
        )
    };
    if required < 16 {
        return None;
    }
    let mut storage = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    let status = unsafe {
        NtQuerySystemInformation(
            SYSTEM_PAGEFILE_INFORMATION,
            storage.as_mut_ptr().cast(),
            (storage.len() * size_of::<usize>()) as u32,
            &mut required,
        )
    };
    if status < 0 {
        return None;
    }
    let bytes = unsafe {
        std::slice::from_raw_parts(
            storage.as_ptr().cast::<u8>(),
            storage.len() * size_of::<usize>(),
        )
    };
    let mut offset = 0usize;
    let mut total_pages = 0u64;
    let mut used_pages = 0u64;
    loop {
        if offset + 16 > bytes.len() {
            return None;
        }
        let next = u32::from_ne_bytes(bytes[offset..offset + 4].try_into().ok()?);
        let total = u32::from_ne_bytes(bytes[offset + 4..offset + 8].try_into().ok()?);
        let used = u32::from_ne_bytes(bytes[offset + 8..offset + 12].try_into().ok()?);
        total_pages = total_pages.saturating_add(u64::from(total));
        used_pages = used_pages.saturating_add(u64::from(used));
        if next == 0 {
            break;
        }
        offset = offset.checked_add(next as usize)?;
    }
    Some((
        total_pages.saturating_mul(page_size),
        used_pages.saturating_mul(page_size),
    ))
}

fn read_memory_composition(total: u64, fallback_available: u64) -> WindowsMemoryComposition {
    const COUNTER_PATHS: [&str; 5] = [
        r"\Memory\Free & Zero Page List Bytes",
        r"\Memory\Modified Page List Bytes",
        r"\Memory\Standby Cache Core Bytes",
        r"\Memory\Standby Cache Normal Priority Bytes",
        r"\Memory\Standby Cache Reserve Bytes",
    ];
    let fallback = || {
        let standby = fallback_available.min(total);
        WindowsMemoryComposition {
            in_use: total.saturating_sub(standby),
            standby,
            ..WindowsMemoryComposition::default()
        }
    };
    let mut query = 0;
    if unsafe { PdhOpenQueryW(ptr::null(), 0, &mut query) } != 0 {
        return fallback();
    }
    struct QueryGuard(Handle);
    impl Drop for QueryGuard {
        fn drop(&mut self) {
            unsafe { PdhCloseQuery(self.0) };
        }
    }
    let query = QueryGuard(query);
    let mut counters = [0; COUNTER_PATHS.len()];
    for (path, counter) in COUNTER_PATHS.iter().zip(&mut counters) {
        let path = wide(path);
        if unsafe { PdhAddEnglishCounterW(query.0, path.as_ptr(), 0, counter) } != 0 {
            return fallback();
        }
    }
    if unsafe { PdhCollectQueryData(query.0) } != 0 {
        return fallback();
    }
    let Some(values) = counters
        .map(read_pdh_large)
        .into_iter()
        .collect::<Option<Vec<_>>>()
    else {
        return fallback();
    };
    let free = values[0].min(total);
    let modified = values[1].min(total.saturating_sub(free));
    let standby = values[2..]
        .iter()
        .copied()
        .fold(0u64, u64::saturating_add)
        .min(total.saturating_sub(free).saturating_sub(modified));
    WindowsMemoryComposition {
        in_use: total
            .saturating_sub(free)
            .saturating_sub(modified)
            .saturating_sub(standby),
        modified,
        standby,
        free,
    }
}

fn read_pdh_large(counter: Handle) -> Option<u64> {
    const PDH_FMT_LARGE: u32 = 0x0000_0400;
    const PDH_CSTATUS_VALID_DATA: u32 = 0;
    const PDH_CSTATUS_NEW_DATA: u32 = 1;
    let mut value_type = 0;
    let mut value = PdhFormattedCounterValue {
        status: u32::MAX,
        value: PdhFormattedValueUnion { large_value: 0 },
    };
    if unsafe { PdhGetFormattedCounterValue(counter, PDH_FMT_LARGE, &mut value_type, &mut value) }
        != 0
        || !matches!(value.status, PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA)
    {
        return None;
    }
    let value = unsafe { value.value.large_value };
    (value >= 0).then_some(value as u64)
}

fn collect_disks(
    config: &Config,
    previous_disks: &mut HashMap<String, DiskCounters>,
    elapsed: f64,
) -> Vec<DiskSample> {
    let required = unsafe { GetLogicalDriveStringsW(0, ptr::null_mut()) };
    if required == 0 {
        return Vec::new();
    }
    let mut buffer = vec![0u16; required as usize + 1];
    if unsafe { GetLogicalDriveStringsW(buffer.len() as u32, buffer.as_mut_ptr()) } == 0 {
        return Vec::new();
    }
    let mut disks = Vec::new();
    let mut current_disks = HashMap::new();
    let only_physical = config.bool_value("only_physical").unwrap_or(true);
    let free_privileged = config.bool_value("disk_free_priv").unwrap_or(false);
    let (exclude_filter, disk_filters) =
        disk_filters(config.value("disks_filter").unwrap_or_default());
    for drive in buffer
        .split(|character| *character == 0)
        .filter(|part| !part.is_empty())
    {
        let mut path = drive.to_vec();
        path.push(0);
        if only_physical && unsafe { GetDriveTypeW(path.as_ptr()) } != DRIVE_FIXED {
            continue;
        }
        let mount = String::from_utf16_lossy(drive);
        if !disk_filters.is_empty() && (disk_filters.contains(&mount) == exclude_filter) {
            continue;
        }
        let mut available = 0;
        let mut total = 0;
        let mut free = 0;
        if unsafe { GetDiskFreeSpaceExW(path.as_ptr(), &mut available, &mut total, &mut free) } == 0
        {
            continue;
        }
        let shown_free = if free_privileged { free } else { available };
        let counters = read_disk_counters(&mount);
        let old = counters.and_then(|_| previous_disks.get(&mount).copied());
        let (read_per_second, write_per_second, io_activity) = old
            .zip(counters)
            .map(|(old, now)| disk_counter_delta(old, now, 1, false, elapsed))
            .unwrap_or_default();
        if let Some(counters) = counters {
            current_disks.insert(mount.clone(), counters);
        }
        disks.push(DiskSample {
            mount,
            total,
            used: total.saturating_sub(shown_free),
            free: shown_free,
            io_supported: counters.is_some(),
            read_per_second,
            write_per_second,
            io_activity,
        });
    }
    *previous_disks = current_disks;
    disks
}

fn read_disk_counters(mount: &str) -> Option<DiskCounters> {
    let volume = mount.trim_end_matches(['\\', '/']);
    let path = wide(&format!(r"\\.\{volume}"));
    let handle = OwnedHandle::new(unsafe {
        CreateFileW(
            path.as_ptr(),
            0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            ptr::null_mut(),
            OPEN_EXISTING,
            0,
            0,
        )
    })?;
    let mut performance = DiskPerformance::default();
    let mut bytes_returned = 0;
    if unsafe {
        DeviceIoControl(
            handle.0,
            IOCTL_DISK_PERFORMANCE,
            ptr::null_mut(),
            0,
            (&mut performance as *mut DiskPerformance).cast(),
            size_of::<DiskPerformance>() as u32,
            &mut bytes_returned,
            ptr::null_mut(),
        )
    } == 0
        || bytes_returned < size_of::<DiskPerformance>() as u32
    {
        return None;
    }
    // DISK_PERFORMANCE times use 100 ns ticks. The shared disk-rate helper
    // expects bytes for a block size of one and cumulative activity in ms.
    Some(DiskCounters {
        sectors_read: performance.bytes_read.max(0) as u64,
        sectors_written: performance.bytes_written.max(0) as u64,
        activity: performance
            .read_time
            .max(0)
            .saturating_add(performance.write_time.max(0)) as u64
            / 10_000,
        activity_valid: true,
    })
}

pub(super) fn collect_network(
    collector: &mut Collector,
    config: &Config,
    elapsed: f64,
) -> Result<NetworkSample, String> {
    let mut table = ptr::null_mut::<MibIfTable2>();
    let result = unsafe { GetIfTable2(&mut table) };
    if result != 0 || table.is_null() {
        return Err(format!("GetIfTable2 failed with Windows error {result}"));
    }
    struct TableGuard(*mut MibIfTable2);
    impl Drop for TableGuard {
        fn drop(&mut self) {
            unsafe { FreeMibTable(self.0.cast()) };
        }
    }
    let table = TableGuard(table);
    let count = unsafe { (*table.0).number_of_entries as usize };
    let first = unsafe { (*table.0).table.as_ptr() };
    let mut rows = Vec::new();
    for index in 0..count {
        let row = unsafe { &*first.add(index) };
        let alias_length = row
            .alias
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(row.alias.len());
        let alias = String::from_utf16_lossy(&row.alias[..alias_length]);
        let filter_interface = row.interface_and_oper_status_flags & 0x02 != 0;
        let unhelpful_virtual_interface = alias.contains("-Npcap Packet Driver")
            || alias.contains("-QoS Packet Scheduler")
            || alias.contains("-WFP ")
            || alias.contains("-Native WiFi Filter Driver")
            || alias.contains("-Virtual WiFi Filter Driver")
            || alias.contains("-Hyper-V Virtual Switch Extension Filter")
            || alias.contains("Kernel Debugger")
            || alias.starts_with("Local Area Connection*")
            || alias.starts_with("vSwitch (");
        let tunnel_or_loopback = matches!(row.interface_type, 24 | 131);
        if filter_interface || unhelpful_virtual_interface || tunnel_or_loopback {
            continue;
        }
        let name = if alias.is_empty() {
            format!("Interface {}", row.interface_index)
        } else {
            alias
        };
        rows.push((
            name,
            row.in_octets,
            row.out_octets,
            row.operational_status,
            row.interface_index,
        ));
    }
    let mut interfaces: Vec<String> = rows.iter().map(|row| row.0.clone()).collect();
    interfaces.sort();
    let mut best_interface = 0;
    let has_best_interface = unsafe {
        // 1.1.1.1 is byte-order invariant and only identifies the best route;
        // no packet is sent by GetBestInterface.
        GetBestInterface(u32::from_ne_bytes([1, 1, 1, 1]), &mut best_interface)
    } == 0;
    let selected = config
        .net_iface
        .as_ref()
        .filter(|selected| interfaces.contains(selected))
        .cloned()
        .or_else(|| {
            has_best_interface
                .then(|| {
                    rows.iter()
                        .find(|row| row.4 == best_interface && row.3 == IF_OPER_STATUS_UP)
                        .map(|row| row.0.clone())
                })
                .flatten()
        })
        .or_else(|| {
            rows.iter()
                .filter(|row| row.3 == IF_OPER_STATUS_UP)
                .max_by_key(|row| row.1.saturating_add(row.2))
                .map(|row| row.0.clone())
        })
        .or_else(|| interfaces.first().cloned())
        .unwrap_or_default();
    let mut selected_values = (0, 0, 0, 0, false);
    let names: HashSet<String> = rows.iter().map(|row| row.0.clone()).collect();
    let addresses = ipv4_addresses();
    let mut selected_index = None;
    for (name, receive, transmit, state, interface_index) in rows {
        let had_previous = collector.previous_network.contains_key(&name);
        let counters = collector.previous_network.entry(name.clone()).or_default();
        let (download, downloaded) = update_network_counter(
            receive,
            &mut counters.receive_last,
            &mut counters.receive_rollover,
            elapsed,
            had_previous,
        );
        let (upload, uploaded) = update_network_counter(
            transmit,
            &mut counters.transmit_last,
            &mut counters.transmit_rollover,
            elapsed,
            had_previous,
        );
        if name == selected {
            selected_index = Some(interface_index);
            selected_values = (
                download,
                upload,
                downloaded,
                uploaded,
                state == IF_OPER_STATUS_UP,
            );
        }
    }
    collector
        .previous_network
        .retain(|name, _| names.contains(name));
    Ok(NetworkSample {
        interfaces,
        selected,
        download_per_second: selected_values.0,
        upload_per_second: selected_values.1,
        downloaded: selected_values.2,
        uploaded: selected_values.3,
        ipv4: selected_index.and_then(|index| addresses.get(&index).cloned()),
        ipv6: None,
        connected: selected_values.4,
    })
}

fn ipv4_addresses() -> HashMap<u32, String> {
    let mut size = 0;
    let _ = unsafe { GetIpAddrTable(ptr::null_mut(), &mut size, 0) };
    if size < size_of::<u32>() as u32 {
        return HashMap::new();
    }
    let mut storage = vec![0u32; (size as usize).div_ceil(size_of::<u32>())];
    if unsafe { GetIpAddrTable(storage.as_mut_ptr().cast(), &mut size, 0) } != 0 {
        return HashMap::new();
    }
    let count = storage[0] as usize;
    let first =
        unsafe { storage.as_ptr().cast::<u8>().add(size_of::<u32>()) }.cast::<MibIpAddrRow>();
    let available = (size as usize - size_of::<u32>()) / size_of::<MibIpAddrRow>();
    let mut addresses = HashMap::new();
    for index in 0..count.min(available) {
        let row = unsafe { *first.add(index) };
        let address = std::net::Ipv4Addr::from(row.address.to_ne_bytes());
        if !address.is_unspecified() && !address.is_loopback() && row.address_type & 0x0048 == 0 {
            let primary = row.address_type & 0x0001 != 0;
            if primary || !addresses.contains_key(&row.interface_index) {
                addresses.insert(row.interface_index, address.to_string());
            }
        }
    }
    addresses
}

pub(super) fn collect_processes(
    collector: &mut Collector,
    total_delta: u64,
    cores: usize,
    config: &Config,
    detailed_pid: Option<u32>,
) -> Result<Vec<ProcessSample>, String> {
    let snapshot = OwnedHandle::new(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) })
        .ok_or_else(|| {
            format!(
                "could not enumerate Windows processes: {}",
                std::io::Error::last_os_error()
            )
        })?;
    let mut now = FileTime::default();
    unsafe { GetSystemTimeAsFileTime(&mut now) };
    let now = now.ticks();
    let mut entry = ProcessEntry32W::default();
    let mut has_entry = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
    let mut next_ticks = HashMap::new();
    let mut processes = Vec::new();
    while has_entry {
        let pid = entry.process_id;
        let name_length = entry
            .exe_file
            .iter()
            .position(|value| *value == 0)
            .unwrap_or(entry.exe_file.len());
        let name = String::from_utf16_lossy(&entry.exe_file[..name_length]);
        let process = OwnedHandle::new(unsafe {
            OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid)
        })
        .or_else(|| {
            OwnedHandle::new(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) })
        });
        let mut ticks = 0u64;
        let mut creation_ticks = now;
        let mut memory = 0u64;
        let mut command = String::new();
        let mut user = String::new();
        let mut nice = base_priority_nice(entry.priority_class_base);
        let mut io = (0, 0);
        if let Some(process) = &process {
            let (mut creation, mut exit, mut kernel, mut user_time) = (
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
                FileTime::default(),
            );
            if unsafe {
                GetProcessTimes(
                    process.0,
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user_time,
                )
            } != 0
            {
                ticks = kernel.ticks().saturating_add(user_time.ticks());
                creation_ticks = creation.ticks();
            }
            let mut counters = MaybeUninit::<ProcessMemoryCounters>::zeroed();
            if unsafe {
                K32GetProcessMemoryInfo(
                    process.0,
                    counters.as_mut_ptr(),
                    size_of::<ProcessMemoryCounters>() as u32,
                )
            } != 0
            {
                memory = unsafe { counters.assume_init() }.working_set_size as u64;
            }
            let mut path = vec![0u16; 32_768];
            let mut length = path.len() as u32;
            if unsafe { QueryFullProcessImageNameW(process.0, 0, path.as_mut_ptr(), &mut length) }
                != 0
            {
                command = String::from_utf16_lossy(&path[..length as usize]);
            }
            if let Some(command_line) = process_command_line(process.0) {
                command = normalize_windows_command_line(&command_line);
            }
            user = process_user(process.0, &mut collector.users);
            let priority_class = unsafe { GetPriorityClass(process.0) };
            if priority_class != 0 {
                nice = priority_class_nice(priority_class);
            }
            if detailed_pid == Some(pid) {
                let mut counters = IoCounters::default();
                if unsafe { GetProcessIoCounters(process.0, &mut counters) } != 0 {
                    io = (counters.read_transfer_count, counters.write_transfer_count);
                }
            }
        }
        next_ticks.insert(pid, ticks);
        let delta = ticks.saturating_sub(
            collector
                .previous_processes
                .get(&pid)
                .copied()
                .unwrap_or(ticks),
        );
        let mut cpu = if total_delta == 0 {
            0.0
        } else {
            delta as f64 * 100.0 / total_delta as f64
        };
        if config.process_per_core {
            cpu *= cores as f64;
        }
        let elapsed_ticks = now.saturating_sub(creation_ticks).max(1);
        processes.push(ProcessSample {
            pid,
            parent: entry.parent_process_id,
            name,
            command,
            user,
            state: '?',
            threads: entry.threads,
            memory,
            cpu: (cpu * 10.0).round() / 10.0,
            cumulative_cpu: ticks as f64 * 100.0 / elapsed_ticks as f64,
            nice,
            kernel_thread: false,
            elapsed_seconds: elapsed_ticks / 10_000_000,
            read_bytes: io.0,
            write_bytes: io.1,
        });
        entry = ProcessEntry32W::default();
        has_entry = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
    }
    collector.previous_processes = next_ticks;
    Ok(processes)
}

fn process_command_line(process: Handle) -> Option<String> {
    let mut required = 0;
    let _ = unsafe {
        NtQueryInformationProcess(
            process,
            PROCESS_COMMAND_LINE_INFORMATION,
            ptr::null_mut(),
            0,
            &mut required,
        )
    };
    if required < size_of::<UnicodeString>() as u32 {
        return None;
    }
    let mut storage = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    let status = unsafe {
        NtQueryInformationProcess(
            process,
            PROCESS_COMMAND_LINE_INFORMATION,
            storage.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    };
    if status < 0 {
        return None;
    }
    let value = unsafe { &*storage.as_ptr().cast::<UnicodeString>() };
    if value.buffer.is_null() || value.length == 0 || value.length % 2 != 0 {
        return None;
    }
    Some(String::from_utf16_lossy(unsafe {
        std::slice::from_raw_parts(value.buffer, usize::from(value.length / 2))
    }))
    .filter(|command| !command.is_empty())
}

fn normalize_windows_command_line(command: &str) -> String {
    let command = command.trim();
    let Some(quoted) = command.strip_prefix('"') else {
        return command.to_string();
    };
    let Some(closing_quote) = quoted.find('"') else {
        return command.to_string();
    };
    let mut normalized = String::with_capacity(command.len().saturating_sub(2));
    normalized.push_str(&quoted[..closing_quote]);
    normalized.push_str(&quoted[closing_quote + 1..]);
    normalized
}

fn process_user(process: Handle, cache: &mut HashMap<u32, String>) -> String {
    let mut raw_token = 0;
    if unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut raw_token) } == 0 {
        return String::new();
    }
    let token = match OwnedHandle::new(raw_token) {
        Some(token) => token,
        None => return String::new(),
    };
    let mut required = 0;
    let _ = unsafe {
        GetTokenInformation(token.0, TOKEN_USER_CLASS, ptr::null_mut(), 0, &mut required)
    };
    if required < size_of::<TokenUser>() as u32 {
        return String::new();
    }
    let mut storage = vec![0usize; (required as usize).div_ceil(size_of::<usize>())];
    if unsafe {
        GetTokenInformation(
            token.0,
            TOKEN_USER_CLASS,
            storage.as_mut_ptr().cast(),
            required,
            &mut required,
        )
    } == 0
    {
        return String::new();
    }
    let sid = unsafe { (*storage.as_ptr().cast::<TokenUser>()).user.sid };
    if sid.is_null() {
        return String::new();
    }
    let sid_length = unsafe { GetLengthSid(sid) } as usize;
    if sid_length == 0 {
        return String::new();
    }
    let sid_bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), sid_length) };
    let cache_key = sid_bytes.iter().fold(2_166_136_261u32, |hash, byte| {
        (hash ^ u32::from(*byte)).wrapping_mul(16_777_619)
    });
    if let Some(user) = cache.get(&cache_key) {
        return user.clone();
    }
    let mut name_length = 0;
    let mut domain_length = 0;
    let mut sid_kind = 0;
    let _ = unsafe {
        LookupAccountSidW(
            ptr::null(),
            sid,
            ptr::null_mut(),
            &mut name_length,
            ptr::null_mut(),
            &mut domain_length,
            &mut sid_kind,
        )
    };
    if name_length == 0 {
        return String::new();
    }
    let mut name = vec![0u16; name_length as usize];
    let mut domain = vec![0u16; domain_length as usize];
    if unsafe {
        LookupAccountSidW(
            ptr::null(),
            sid,
            name.as_mut_ptr(),
            &mut name_length,
            domain.as_mut_ptr(),
            &mut domain_length,
            &mut sid_kind,
        )
    } == 0
    {
        return String::new();
    }
    let end = name[..name_length as usize]
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(name_length as usize);
    let user = String::from_utf16_lossy(&name[..end]);
    cache.insert(cache_key, user.clone());
    user
}

fn base_priority_nice(priority: i32) -> i32 {
    match priority {
        ..=4 => 10,
        5..=7 => 1,
        8 => 0,
        9..=11 => -1,
        12..=15 => -10,
        _ => -20,
    }
}

fn priority_class_nice(priority: u32) -> i32 {
    match priority {
        0x0000_0040 => 10,  // IDLE_PRIORITY_CLASS
        0x0000_4000 => 1,   // BELOW_NORMAL_PRIORITY_CLASS
        0x0000_0020 => 0,   // NORMAL_PRIORITY_CLASS
        0x0000_8000 => -1,  // ABOVE_NORMAL_PRIORITY_CLASS
        0x0000_0080 => -10, // HIGH_PRIORITY_CLASS
        0x0000_0100 => -20, // REALTIME_PRIORITY_CLASS
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn windows_abi_layouts_match_sdk() {
        assert_eq!(size_of::<ProcessorPerformanceInfo>(), 48);
        assert_eq!(size_of::<MemoryStatusEx>(), 64);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<PerformanceInfo>(), 104);
        assert_eq!(size_of::<DiskPerformance>(), 88);
        assert_eq!(size_of::<MibIfRow2>(), 1_352);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<ProcessEntry32W>(), 568);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<UnicodeString>(), 16);
        #[cfg(target_pointer_width = "64")]
        assert_eq!(size_of::<TokenUser>(), 16);
        assert_eq!(size_of::<PdhFormattedCounterValue>(), 16);
    }

    #[test]
    fn displayed_windows_commands_do_not_mix_executable_quote_styles() {
        assert_eq!(
            normalize_windows_command_line(
                r#""C:\Program Files\Example\app.exe" --name "two words""#
            ),
            r#"C:\Program Files\Example\app.exe --name "two words""#
        );
        assert_eq!(
            normalize_windows_command_line(r#""C:\Windows\explorer.exe""#),
            r"C:\Windows\explorer.exe"
        );
        assert_eq!(
            normalize_windows_command_line(r"C:\Windows\System32\cmd.exe /c echo"),
            r"C:\Windows\System32\cmd.exe /c echo"
        );
    }

    #[test]
    #[ignore = "requires physical Windows hardware, an active IPv4 adapter, and disk counters"]
    fn live_windows_collectors_return_core_system_data() {
        let config = Config::default();
        let mut collector = Collector::new(&config).unwrap();
        std::thread::sleep(Duration::from_millis(1_100));
        let sample = collector.collect(&config, None).unwrap();
        assert!(!sample.cpu.cores.is_empty());
        assert!(!sample.cpu.name.is_empty());
        assert!(sample.cpu.load.iter().all(|value| value.is_nan()));
        assert!(!sample.cpu.frequency.is_empty());
        assert!(sample.cpu.core_frequencies_mhz.is_empty());
        assert!(sample.memory.total > 0);
        assert!(sample.memory.swap_used <= sample.memory.swap_total);
        assert_eq!(
            sample.memory.used + sample.memory.modified + sample.memory.cached + sample.memory.free,
            sample.memory.total
        );
        assert!(!sample.memory.disks.is_empty());
        assert!(
            sample.memory.disks.iter().any(|disk| disk.io_supported),
            "no Windows volume exposed disk performance counters: {:?}",
            sample
                .memory
                .disks
                .iter()
                .map(|disk| &disk.mount)
                .collect::<Vec<_>>()
        );
        assert!(!sample.processes.is_empty());
        let current = sample
            .processes
            .iter()
            .find(|process| process.pid == std::process::id())
            .expect("collector includes its own process");
        assert!(!current.command.is_empty());
        assert!(!current.user.is_empty());
        assert_eq!(current.state, '?');
        assert!(!sample.network.interfaces.is_empty());
        assert!(sample.network.interfaces.len() < 16);
        assert!(sample.network.interfaces.iter().all(|name| {
            !name.contains("Npcap")
                && !name.contains("QoS Packet Scheduler")
                && !name.contains("-WFP ")
        }));
        assert!(!sample.network.selected.is_empty());
        assert!(
            sample.network.ipv4.is_some(),
            "default-route adapter {} has no IPv4 address",
            sample.network.selected
        );
        assert!(
            sample.network.downloaded > 0 || sample.network.uploaded > 0,
            "selected adapter {} reported no lifetime traffic",
            sample.network.selected
        );
    }
}
