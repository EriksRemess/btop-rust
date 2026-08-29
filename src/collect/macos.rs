use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString, c_char, c_int, c_uint, c_void};
use std::mem;
use std::net::Ipv6Addr;
use std::ptr;
use std::time::{SystemTime, UNIX_EPOCH};

use super::{
    BatterySample, Collector, CpuSample, CpuTicks, DiskSample, MemorySample, NetworkSample,
    ProcessSample, update_network_counter,
};
use crate::config::Config;

const KERN_SUCCESS: c_int = 0;
const PROCESSOR_CPU_LOAD_INFO: c_int = 2;
const CPU_STATE_USER: usize = 0;
const CPU_STATE_SYSTEM: usize = 1;
const CPU_STATE_IDLE: usize = 2;
const CPU_STATE_NICE: usize = 3;
const HOST_VM_INFO64: c_int = 4;
const PROC_PIDTBSDINFO: c_int = 3;
const PROC_PIDTASKINFO: c_int = 4;
const PROC_PIDT_SHORTBSDINFO: c_int = 13;
const PROC_FLAG_SYSTEM: u32 = 1;
const CTL_KERN: c_int = 1;
const CTL_NET: c_int = 4;
const KERN_PROCARGS2: c_int = 49;
const PF_ROUTE: c_int = 17;
const NET_RT_IFLIST2: c_int = 6;
const RTM_IFINFO2: u8 = 0x12;
const AF_INET: u8 = 2;
const AF_INET6: u8 = 30;
const IFF_RUNNING: c_uint = 0x40;
const MNT_NOWAIT: c_int = 2;
const SC_CLK_TCK_DARWIN: c_int = 3;

pub(super) fn read_cpu_name() -> String {
    sysctl_string("machdep.cpu.brand_string")
        .or_else(|| sysctl_string("hw.model"))
        .map(|name| super::clean_cpu_name(name.trim()))
        .unwrap_or_else(|| "Apple CPU".to_string())
}

pub(super) fn collect_cpu(
    collector: &mut Collector,
    config: &Config,
) -> Result<(CpuSample, u64), String> {
    let current = processor_ticks(&collector.previous_cpu)?;
    let percentages = current
        .iter()
        .enumerate()
        .map(|(index, now)| {
            let old = collector
                .previous_cpu
                .get(index)
                .copied()
                .unwrap_or_default();
            percent(
                now.busy.saturating_sub(old.busy),
                now.total.saturating_sub(old.total),
            )
        })
        .collect::<Vec<_>>();
    let old_total = collector.previous_cpu.first().copied().unwrap_or_default();
    let total_delta = current
        .first()
        .map(|now| now.total.saturating_sub(old_total.total))
        .unwrap_or(0);
    let fields = current
        .first()
        .map(|now| {
            ["user", "nice", "system", "idle"]
                .into_iter()
                .enumerate()
                .map(|(index, name)| {
                    let delta = now.fields[index].saturating_sub(old_total.fields[index]);
                    (name.to_string(), percent(delta, total_delta).round())
                })
                .collect()
        })
        .unwrap_or_default();
    collector.previous_cpu = current;

    let mut load = [0.0; 3];
    unsafe {
        getloadavg(load.as_mut_ptr(), load.len() as c_int);
    }
    let core_count = percentages.len().saturating_sub(1);
    let (temperature, core_temperatures) = cpu_temperatures(config.check_temperature, core_count);
    Ok((
        CpuSample {
            total: percentages.first().copied().unwrap_or(0.0),
            fields,
            cores: percentages.into_iter().skip(1).collect(),
            load,
            frequency: config
                .show_cpu_frequency
                .then(read_frequency)
                .flatten()
                .unwrap_or_default(),
            temperature,
            temperature_max: 95.0,
            core_temperatures,
            name: collector.cpu_name.clone(),
            uptime: read_uptime(),
            battery: config
                .bool_value("show_battery")
                .unwrap_or(true)
                .then(read_battery)
                .flatten(),
            watts: None,
            container_engine: None,
            active_cpus: Some((0..core_count).collect::<HashSet<_>>()),
            available_batteries: vec!["Auto".to_string()],
        },
        total_delta,
    ))
}

fn cpu_temperatures(enabled: bool, core_count: usize) -> (Option<f64>, Vec<Option<f64>>) {
    if !enabled {
        return (None, vec![None; core_count]);
    }
    #[cfg(target_arch = "aarch64")]
    {
        crate::gpu::macos::read_cpu_temperatures(core_count)
    }
    #[cfg(not(target_arch = "aarch64"))]
    {
        (None, vec![None; core_count])
    }
}

fn extend_u32_counter(current: u32, previous: u64) -> u64 {
    let mut extended = (previous & !u64::from(u32::MAX)) | u64::from(current);
    if extended < previous {
        extended = extended.saturating_add(1_u64 << 32);
    }
    extended
}

fn processor_ticks(previous: &[CpuTicks]) -> Result<Vec<CpuTicks>, String> {
    let mut cpu_count = 0_u32;
    let mut info = ptr::null_mut::<c_int>();
    let mut info_count = 0_u32;
    let result = unsafe {
        host_processor_info(
            mach_host_self(),
            PROCESSOR_CPU_LOAD_INFO,
            &mut cpu_count,
            &mut info,
            &mut info_count,
        )
    };
    if result != KERN_SUCCESS || info.is_null() {
        return Err(format!(
            "host_processor_info failed with Mach error {result}"
        ));
    }
    let values = unsafe { std::slice::from_raw_parts(info, info_count as usize) };
    let mut aggregate = CpuTicks::default();
    let mut cores = Vec::with_capacity(cpu_count as usize + 1);
    for (core_index, values) in values
        .as_chunks::<4>()
        .0
        .iter()
        .take(cpu_count as usize)
        .enumerate()
    {
        let old = previous.get(core_index + 1).copied().unwrap_or_default();
        let user = extend_u32_counter(values[CPU_STATE_USER] as u32, old.fields[0]);
        let nice = extend_u32_counter(values[CPU_STATE_NICE] as u32, old.fields[1]);
        let system = extend_u32_counter(values[CPU_STATE_SYSTEM] as u32, old.fields[2]);
        let idle = extend_u32_counter(values[CPU_STATE_IDLE] as u32, old.fields[3]);
        let total = user + nice + system + idle;
        let ticks = CpuTicks {
            busy: total.saturating_sub(idle),
            total,
            fields: [user, nice, system, idle, 0, 0, 0, 0, 0, 0],
        };
        aggregate.busy = aggregate.busy.saturating_add(ticks.busy);
        aggregate.total = aggregate.total.saturating_add(ticks.total);
        for (sum, value) in aggregate.fields.iter_mut().zip(ticks.fields) {
            *sum = sum.saturating_add(value);
        }
        cores.push(ticks);
    }
    unsafe {
        vm_deallocate(
            mach_task_self_,
            info as usize,
            info_count as usize * mem::size_of::<c_int>(),
        );
    }
    cores.insert(0, aggregate);
    Ok(cores)
}

fn percent(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (value as f64 * 100.0 / total as f64).clamp(0.0, 100.0)
    }
}

fn read_frequency() -> Option<String> {
    let hz = sysctl_value::<u64>("hw.cpufrequency")?;
    Some(super::normalize_frequency(hz as f64 / 1_000_000.0))
}

fn read_uptime() -> u64 {
    let Some(boot) = sysctl_value::<Timeval>("kern.boottime") else {
        return 0;
    };
    let Some(now) = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_secs())
    else {
        return 0;
    };
    now.saturating_sub(boot.seconds.max(0) as u64)
}

pub(super) fn collect_memory(
    config: &Config,
    previous_disks: &mut HashMap<String, super::DiskCounters>,
    elapsed: f64,
) -> Result<MemorySample, String> {
    let total = sysctl_value::<u64>("hw.memsize").unwrap_or(0);
    let page_size = sysctl_value::<u64>("hw.pagesize").unwrap_or(4096);
    let mut stats = VmStatistics64::default();
    let mut count = (mem::size_of::<VmStatistics64>() / mem::size_of::<c_int>()) as u32;
    let result = unsafe {
        host_statistics64(
            mach_host_self(),
            HOST_VM_INFO64,
            (&mut stats as *mut VmStatistics64).cast(),
            &mut count,
        )
    };
    if result != KERN_SUCCESS {
        return Err(format!("host_statistics64 failed with Mach error {result}"));
    }
    let free = u64::from(stats.free_count).saturating_mul(page_size);
    let cached = u64::from(stats.external_page_count).saturating_mul(page_size);
    // Apple's internal/external counters describe backing type, while the
    // active/inactive counters describe page-queue state and therefore overlap.
    // Treat anonymous, wired and compressor storage as used; file-backed pages
    // remain reclaimable cache. Purgeable pages are reclaimable as well.
    let used_pages = macos_used_pages(&stats);
    let used = used_pages.saturating_mul(page_size).min(total);
    let available = total.saturating_sub(used);
    let swap = sysctl_value::<XswUsage>("vm.swapusage").unwrap_or_default();
    Ok(MemorySample {
        total,
        used,
        free,
        available,
        cached,
        swap_total: swap.total,
        swap_used: swap.used,
        disks: if config.show_disks {
            collect_disks(config, previous_disks, elapsed)
        } else {
            Vec::new()
        },
    })
}

fn macos_used_pages(stats: &VmStatistics64) -> u64 {
    u64::from(stats.internal_page_count)
        .saturating_add(u64::from(stats.wire_count))
        .saturating_add(u64::from(stats.compressor_page_count))
        .saturating_sub(u64::from(stats.purgeable_count))
}

fn collect_disks(
    config: &Config,
    previous: &mut HashMap<String, super::DiskCounters>,
    elapsed: f64,
) -> Vec<DiskSample> {
    let mut mounts = ptr::null_mut::<StatFs>();
    let count = unsafe { getmntinfo(&mut mounts, MNT_NOWAIT) };
    if count <= 0 || mounts.is_null() {
        return Vec::new();
    }
    let free_priv = config.bool_value("disk_free_priv").unwrap_or(false);
    let (exclude, filters) = super::disk_filters(config.value("disks_filter").unwrap_or_default());
    let mut seen = HashSet::new();
    let mut mappings = HashMap::new();
    let mut disks = unsafe { std::slice::from_raw_parts(mounts, count as usize) }
        .iter()
        .filter_map(|stats| {
            let filesystem = c_array_string(&stats.filesystem);
            let mount = c_array_string(&stats.mountpoint);
            let device = c_array_string(&stats.mounted_from);
            if !device.is_empty() && !mount.is_empty() {
                mappings.insert(device, mount.clone());
            }
            if mount.is_empty()
                || matches!(filesystem.as_str(), "autofs" | "devfs")
                || !seen.insert(mount.clone())
                || (!filters.is_empty() && (filters.contains(&mount) == exclude))
            {
                return None;
            }
            let total = stats.blocks.saturating_mul(u64::from(stats.block_size));
            let free_blocks = if free_priv {
                stats.blocks_free
            } else {
                stats.blocks_available
            };
            let free = free_blocks.saturating_mul(u64::from(stats.block_size));
            Some(DiskSample {
                mount,
                total,
                used: total.saturating_sub(free),
                free,
                io_supported: false,
                read_per_second: 0,
                write_per_second: 0,
                io_activity: 0.0,
            })
        })
        .collect::<Vec<_>>();
    if let Some(root) = disks.iter().position(|disk| disk.mount == "/") {
        let root = disks.remove(root);
        disks.insert(0, root);
    }
    collect_disk_io(&mut disks, &mappings, previous, elapsed);
    disks
}

fn collect_disk_io(
    disks: &mut [DiskSample],
    mappings: &HashMap<String, String>,
    previous: &mut HashMap<String, super::DiskCounters>,
    elapsed: f64,
) {
    let Ok(class) = CString::new("IOMediaBSDClient") else {
        return;
    };
    let matching = unsafe { IOServiceMatching(class.as_ptr()) };
    let mut iterator = 0_u32;
    if matching.is_null()
        || unsafe { IOServiceGetMatchingServices(0, matching, &mut iterator) } != KERN_SUCCESS
    {
        return;
    }
    loop {
        let drive = unsafe { IOIteratorNext(iterator) };
        if drive == 0 {
            break;
        }
        let mut volume = 0_u32;
        let got_parent =
            unsafe { IORegistryEntryGetParentEntry(drive, c"IOService".as_ptr(), &mut volume) }
                == KERN_SUCCESS;
        if got_parent && volume != 0 && !io_registry_bool(volume, "Whole").unwrap_or(true) {
            let bsd_name = io_registry_string(volume, "BSD Name").unwrap_or_default();
            let mut device = io_registry_string(volume, "VolGroupMntFromName").unwrap_or_default();
            if !mappings.contains_key(&device) && !bsd_name.is_empty() {
                device = format!("/dev/{bsd_name}");
            }
            if let Some(mount) = mappings.get(&device)
                && let Some(disk) = disks.iter_mut().find(|disk| &disk.mount == mount)
                && let Some((read, write, total_time)) = io_registry_disk_statistics(volume)
            {
                let current = super::DiskCounters {
                    sectors_read: read,
                    sectors_written: write,
                    activity: total_time.unwrap_or_default(),
                    activity_valid: total_time.is_some(),
                };
                let old = previous.insert(mount.clone(), current);
                disk.io_supported = true;
                if let Some(old) = old {
                    let (read_per_second, write_per_second, activity) =
                        macos_disk_counter_delta(old, current, elapsed);
                    disk.read_per_second = read_per_second;
                    disk.write_per_second = write_per_second;
                    disk.io_activity = activity;
                }
            }
        }
        if volume != 0 {
            unsafe { IOObjectRelease(volume) };
        }
        unsafe { IOObjectRelease(drive) };
    }
    unsafe { IOObjectRelease(iterator) };
    previous.retain(|mount, _| disks.iter().any(|disk| &disk.mount == mount));
}

fn io_registry_property(entry: u32, key: &str) -> Option<*const c_void> {
    let key = CString::new(key).ok()?;
    let cf_key = unsafe { CFStringCreateWithCString(ptr::null(), key.as_ptr(), 0x0800_0100) };
    if cf_key.is_null() {
        return None;
    }
    let value = unsafe { IORegistryEntryCreateCFProperty(entry, cf_key, ptr::null(), 0) };
    unsafe { CFRelease(cf_key) };
    (!value.is_null()).then_some(value)
}

fn io_registry_bool(entry: u32, key: &str) -> Option<bool> {
    let value = io_registry_property(entry, key)?;
    let result =
        unsafe { cf_is_type(value, CFBooleanGetTypeID()).then(|| CFBooleanGetValue(value)) };
    unsafe { CFRelease(value) };
    result
}

fn io_registry_string(entry: u32, key: &str) -> Option<String> {
    let value = io_registry_property(entry, key)?;
    if unsafe { !cf_is_type(value, CFStringGetTypeID()) } {
        unsafe { CFRelease(value) };
        return None;
    }
    let mut buffer = [0_i8; 1024];
    let success = unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            0x0800_0100,
        )
    };
    unsafe { CFRelease(value) };
    success.then(|| {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

fn macos_disk_counter_delta(
    old: super::DiskCounters,
    now: super::DiskCounters,
    elapsed: f64,
) -> (u64, u64, f64) {
    let seconds = elapsed.max(0.001);
    let read = (now.sectors_read.saturating_sub(old.sectors_read) as f64 / seconds).round() as u64;
    let write =
        (now.sectors_written.saturating_sub(old.sectors_written) as f64 / seconds).round() as u64;
    let activity = if old.activity_valid && now.activity_valid {
        let busy_nanoseconds = now.activity.saturating_sub(old.activity) as f64;
        (busy_nanoseconds / (seconds * 1_000_000_000.0) * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    (read, write, activity)
}

fn io_registry_disk_statistics(entry: u32) -> Option<(u64, u64, Option<u64>)> {
    let mut properties = ptr::null();
    if unsafe { IORegistryEntryCreateCFProperties(entry, &mut properties, ptr::null(), 0) }
        != KERN_SUCCESS
        || properties.is_null()
    {
        return None;
    }
    let statistics = unsafe { cf_dictionary_value(properties, "Statistics") };
    let result = statistics.and_then(|statistics| unsafe {
        let read = cf_dictionary_u64(statistics, "Bytes read from block device")
            .or_else(|| cf_dictionary_u64(statistics, "Bytes (Read)"))?;
        let write = cf_dictionary_u64(statistics, "Bytes written to block device")
            .or_else(|| cf_dictionary_u64(statistics, "Bytes (Write)"))?;
        Some((read, write))
    });
    unsafe { CFRelease(properties) };
    let (read, write) = result?;
    Some((read, write, io_registry_ancestor_total_time(entry)))
}

fn io_registry_ancestor_total_time(entry: u32) -> Option<u64> {
    let mut current = 0_u32;
    if unsafe { IORegistryEntryGetParentEntry(entry, c"IOService".as_ptr(), &mut current) }
        != KERN_SUCCESS
        || current == 0
    {
        return None;
    }
    for _ in 0..64 {
        if let Some(total) = io_registry_total_time(current) {
            unsafe { IOObjectRelease(current) };
            return Some(total);
        }
        let mut parent = 0_u32;
        let got_parent =
            unsafe { IORegistryEntryGetParentEntry(current, c"IOService".as_ptr(), &mut parent) }
                == KERN_SUCCESS;
        unsafe { IOObjectRelease(current) };
        if !got_parent || parent == 0 {
            return None;
        }
        current = parent;
    }
    unsafe { IOObjectRelease(current) };
    None
}

fn io_registry_total_time(entry: u32) -> Option<u64> {
    let mut properties = ptr::null();
    if unsafe { IORegistryEntryCreateCFProperties(entry, &mut properties, ptr::null(), 0) }
        != KERN_SUCCESS
        || properties.is_null()
    {
        return None;
    }
    let result = unsafe {
        cf_dictionary_value(properties, "Statistics").and_then(|statistics| {
            let read = cf_dictionary_u64(statistics, "Total Time (Read)");
            let write = cf_dictionary_u64(statistics, "Total Time (Write)");
            match (read, write) {
                (None, None) => None,
                (read, write) => Some(
                    read.unwrap_or_default()
                        .saturating_add(write.unwrap_or_default()),
                ),
            }
        })
    };
    unsafe { CFRelease(properties) };
    result
}

pub(super) fn collect_network(
    collector: &mut Collector,
    config: &Config,
    elapsed: f64,
) -> Result<NetworkSample, String> {
    let mut head = ptr::null_mut::<IfAddrs>();
    if unsafe { getifaddrs(&mut head) } != 0 {
        return Err(format!(
            "getifaddrs failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut found: HashMap<String, Interface> = HashMap::new();
    let mut current = head;
    while !current.is_null() {
        let address = unsafe { &*current };
        if !address.name.is_null() {
            let name = unsafe { CStr::from_ptr(address.name) }
                .to_string_lossy()
                .into_owned();
            let entry = found.entry(name).or_default();
            entry.connected |= address.flags & IFF_RUNNING != 0;
            if !address.address.is_null() {
                let socket = unsafe { ptr::read_unaligned(address.address) };
                match socket.family {
                    AF_INET if entry.ipv4.is_none() => {
                        let socket =
                            unsafe { ptr::read_unaligned(address.address.cast::<SockAddrIn>()) };
                        let bytes = socket.address.to_ne_bytes();
                        entry.ipv4 = Some(format!(
                            "{}.{}.{}.{}",
                            bytes[0], bytes[1], bytes[2], bytes[3]
                        ));
                    }
                    AF_INET6 if entry.ipv6.is_none() => {
                        let socket =
                            unsafe { ptr::read_unaligned(address.address.cast::<SockAddrIn6>()) };
                        entry.ipv6 = Some(Ipv6Addr::from(socket.address).to_string());
                    }
                    _ => {}
                }
            }
        }
        current = address.next;
    }
    unsafe { freeifaddrs(head) };

    for (name, (received, transmitted)) in interface_counters64()? {
        let entry = found.entry(name).or_default();
        entry.received = received;
        entry.transmitted = transmitted;
    }

    let mut interfaces = found.keys().cloned().collect::<Vec<_>>();
    interfaces.sort();
    let selected = config
        .net_iface
        .as_ref()
        .filter(|name| found.contains_key(*name))
        .cloned()
        .or_else(|| primary_network_interface().filter(|name| found.contains_key(name)))
        .or_else(|| {
            interfaces
                .iter()
                .filter(|name| found.get(*name).is_some_and(|value| value.connected))
                .max_by_key(|name| {
                    let value = found.get(*name).cloned().unwrap_or_default();
                    value.received.saturating_add(value.transmitted)
                })
                .cloned()
        })
        .or_else(|| interfaces.first().cloned())
        .unwrap_or_default();
    let mut speeds = HashMap::new();
    let mut totals = HashMap::new();
    for (name, value) in &found {
        let had_previous = collector.previous_network.contains_key(name);
        let saved = collector.previous_network.entry(name.clone()).or_default();
        let (receive_speed, receive_total) = update_network_counter(
            value.received,
            &mut saved.receive_last,
            &mut saved.receive_rollover,
            elapsed,
            had_previous,
        );
        let (transmit_speed, transmit_total) = update_network_counter(
            value.transmitted,
            &mut saved.transmit_last,
            &mut saved.transmit_rollover,
            elapsed,
            had_previous,
        );
        speeds.insert(name.clone(), (receive_speed, transmit_speed));
        totals.insert(name.clone(), (receive_total, transmit_total));
    }
    collector
        .previous_network
        .retain(|name, _| found.contains_key(name));
    let value = found.get(&selected).cloned().unwrap_or_default();
    let (download_per_second, upload_per_second) =
        speeds.get(&selected).copied().unwrap_or_default();
    let (downloaded, uploaded) = totals.get(&selected).copied().unwrap_or_default();
    Ok(NetworkSample {
        interfaces,
        selected,
        download_per_second,
        upload_per_second,
        downloaded,
        uploaded,
        ipv4: value.ipv4,
        ipv6: value.ipv6,
        connected: value.connected,
    })
}

fn interface_counters64() -> Result<HashMap<String, (u64, u64)>, String> {
    let mut mib = [CTL_NET, PF_ROUTE, 0, 0, NET_RT_IFLIST2, 0];
    let mut size = 0_usize;
    if unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as c_uint,
            ptr::null_mut(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(format!(
            "NET_RT_IFLIST2 size query failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let mut buffer = vec![0_u8; size];
    if unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as c_uint,
            buffer.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
    {
        return Err(format!(
            "NET_RT_IFLIST2 query failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    buffer.truncate(size);

    let mut counters = HashMap::new();
    let mut offset = 0_usize;
    while offset + mem::size_of::<RouteMessageHeader>() <= buffer.len() {
        let header = unsafe {
            ptr::read_unaligned(buffer.as_ptr().add(offset).cast::<RouteMessageHeader>())
        };
        let message_length = usize::from(header.length);
        if message_length == 0 || offset.saturating_add(message_length) > buffer.len() {
            break;
        }
        if header.kind == RTM_IFINFO2 && message_length >= mem::size_of::<IfMessageHeader2>() {
            let message = unsafe {
                ptr::read_unaligned(buffer.as_ptr().add(offset).cast::<IfMessageHeader2>())
            };
            let mut name = [0_i8; 16];
            if unsafe { !if_indextoname(u32::from(message.index), name.as_mut_ptr()).is_null() } {
                counters.insert(
                    unsafe { CStr::from_ptr(name.as_ptr()) }
                        .to_string_lossy()
                        .into_owned(),
                    (message.data.bytes_received, message.data.bytes_transmitted),
                );
            }
        }
        offset += message_length;
    }
    Ok(counters)
}

fn primary_network_interface() -> Option<String> {
    for path in ["State:/Network/Global/IPv4", "State:/Network/Global/IPv6"] {
        let key = cf_string(path)?;
        let value = unsafe { SCDynamicStoreCopyValue(ptr::null(), key) };
        unsafe { CFRelease(key) };
        if value.is_null() {
            continue;
        }
        let interface = unsafe {
            cf_is_type(value, CFDictionaryGetTypeID())
                .then(|| cf_dictionary_value(value, "PrimaryInterface"))
                .flatten()
                .and_then(|name| cf_string_to_string(name))
        };
        unsafe { CFRelease(value) };
        if interface.is_some() {
            return interface;
        }
    }
    None
}

pub(super) fn collect_processes(
    collector: &mut Collector,
    total_delta: u64,
    cores: usize,
    config: &Config,
    detailed_pid: Option<u32>,
) -> Result<Vec<ProcessSample>, String> {
    let count = unsafe { proc_listallpids(ptr::null_mut(), 0) };
    if count <= 0 {
        return Err(format!(
            "proc_listallpids failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    // proc_listallpids() returns a PID count (its buffer size is still bytes).
    // Keep headroom for processes created between the sizing and fill calls.
    let mut pids = vec![0_i32; count as usize + 32];
    let count = unsafe {
        proc_listallpids(
            pids.as_mut_ptr().cast(),
            (pids.len() * mem::size_of::<c_int>()) as c_int,
        )
    };
    if count < 0 {
        return Err(format!(
            "proc_listallpids failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    pids.truncate((count as usize).min(pids.len()));
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let timebase = mach_timebase_ns_per_tick();
    let clock_ticks = unsafe { sysconf(SC_CLK_TCK_DARWIN) }.max(1) as f64;
    let interval_ns = total_delta.max(1) as f64 * 1_000_000_000.0 / clock_ticks;
    let mut next_times = HashMap::new();
    let mut processes = Vec::with_capacity(pids.len());
    let argument_limit = sysctl_value::<c_int>("kern.argmax")
        .unwrap_or(4096)
        .clamp(4096, 1 << 20) as usize;
    let mut argument_buffer = vec![0_u8; argument_limit];
    for pid in pids.into_iter().filter(|pid| *pid > 0) {
        let mut bsd = ProcBsdInfo::default();
        let got_full_bsd = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDTBSDINFO,
                0,
                (&mut bsd as *mut ProcBsdInfo).cast(),
                mem::size_of::<ProcBsdInfo>() as c_int,
            )
        } == mem::size_of::<ProcBsdInfo>() as c_int;
        if !got_full_bsd {
            // macOS denies the full BSD record for a number of protected
            // processes, but still exposes the short record. Keep those
            // processes in the list with the metadata that is available.
            let mut short = ProcBsdShortInfo::default();
            if unsafe {
                proc_pidinfo(
                    pid,
                    PROC_PIDT_SHORTBSDINFO,
                    0,
                    (&mut short as *mut ProcBsdShortInfo).cast(),
                    mem::size_of::<ProcBsdShortInfo>() as c_int,
                )
            } != mem::size_of::<ProcBsdShortInfo>() as c_int
            {
                continue;
            }
            bsd.pid = short.pid;
            bsd.parent_pid = short.parent_pid;
            bsd.process_group = short.process_group;
            bsd.status = short.status;
            bsd.flags = short.flags;
            bsd.uid = short.uid;
            bsd.gid = short.gid;
            bsd.real_uid = short.real_uid;
            bsd.real_gid = short.real_gid;
            bsd.saved_uid = short.saved_uid;
            bsd.saved_gid = short.saved_gid;
            bsd.command = short.command;
        }
        let mut task = ProcTaskInfo::default();
        let got_task = unsafe {
            proc_pidinfo(
                pid,
                PROC_PIDTASKINFO,
                0,
                (&mut task as *mut ProcTaskInfo).cast(),
                mem::size_of::<ProcTaskInfo>() as c_int,
            )
        } == mem::size_of::<ProcTaskInfo>() as c_int;
        let cpu_time = if got_task {
            task.total_user.saturating_add(task.total_system)
        } else {
            collector
                .previous_processes
                .get(&(pid as u32))
                .copied()
                .unwrap_or(0)
        };
        next_times.insert(pid as u32, cpu_time);
        let delta = collector
            .previous_processes
            .get(&(pid as u32))
            .map(|old| cpu_time.saturating_sub(*old))
            .unwrap_or(0);
        let mut cpu = delta as f64 * timebase * 100.0 / interval_ns;
        if config.process_per_core {
            cpu *= cores.max(1) as f64;
        }
        let start = bsd.start_seconds as f64 + bsd.start_microseconds as f64 / 1_000_000.0;
        let elapsed_seconds = if got_full_bsd {
            (now - start).max(0.0) as u64
        } else {
            0
        };
        let cumulative_cpu = if got_full_bsd && now > start {
            cpu_time as f64 * timebase * 100.0 / ((now - start) * 1_000_000_000.0)
        } else {
            0.0
        };
        let name = proc_name_string(pid)
            .filter(|name| !name.is_empty())
            .or_else(|| c_array_opt(&bsd.name))
            .or_else(|| c_array_opt(&bsd.command))
            .unwrap_or_else(|| pid.to_string());
        let command = process_arguments(pid, &mut argument_buffer)
            .unwrap_or_else(|| process_path(pid).unwrap_or_else(|| name.clone()));
        let (read_bytes, write_bytes) = if detailed_pid == Some(pid as u32) {
            process_io(pid)
        } else {
            (0, 0)
        };
        let user = collector.users.get(&bsd.uid).cloned().unwrap_or_else(|| {
            let name = user_name(bsd.uid).unwrap_or_else(|| bsd.uid.to_string());
            collector.users.insert(bsd.uid, name.clone());
            name
        });
        processes.push(ProcessSample {
            pid: pid as u32,
            parent: bsd.parent_pid,
            name,
            command,
            user,
            state: process_state(bsd.status),
            threads: if got_task {
                task.thread_count.max(0) as u32
            } else {
                0
            },
            memory: if got_task { task.resident_size } else { 0 },
            cpu: (cpu * 10.0).round() / 10.0,
            cumulative_cpu,
            nice: bsd.nice,
            kernel_thread: bsd.flags & PROC_FLAG_SYSTEM != 0,
            elapsed_seconds,
            read_bytes,
            write_bytes,
        });
    }
    collector.previous_processes = next_times;
    Ok(processes)
}

fn mach_timebase_ns_per_tick() -> f64 {
    let mut info = MachTimebaseInfo::default();
    if unsafe { mach_timebase_info(&mut info) } == KERN_SUCCESS && info.denom > 0 {
        info.numer as f64 / info.denom as f64
    } else {
        1.0
    }
}

fn process_state(status: u32) -> char {
    match status {
        1 => 'I',
        2 => 'R',
        3 => 'S',
        4 => 'T',
        5 => 'Z',
        _ => '?',
    }
}

fn user_name(uid: u32) -> Option<String> {
    let password = unsafe { getpwuid(uid) };
    if password.is_null() {
        return None;
    }
    let name = unsafe { (*password).name };
    (!name.is_null()).then(|| {
        unsafe { CStr::from_ptr(name) }
            .to_string_lossy()
            .into_owned()
    })
}

fn proc_name_string(pid: c_int) -> Option<String> {
    let mut buffer = [0_i8; 256];
    let length = unsafe { proc_name(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    (length > 0).then(|| {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

fn process_path(pid: c_int) -> Option<String> {
    let mut buffer = [0_i8; 4096];
    let length = unsafe { proc_pidpath(pid, buffer.as_mut_ptr().cast(), buffer.len() as u32) };
    (length > 0).then(|| {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

fn process_arguments(pid: c_int, buffer: &mut [u8]) -> Option<String> {
    let mut size = buffer.len();
    let mut mib = [CTL_KERN, KERN_PROCARGS2, pid];
    if unsafe {
        sysctl(
            mib.as_mut_ptr(),
            mib.len() as c_uint,
            buffer.as_mut_ptr().cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } != 0
        || size <= mem::size_of::<c_int>()
    {
        return None;
    }
    let buffer = buffer.get(..size)?;
    let argc = c_int::from_ne_bytes(buffer.get(..4)?.try_into().ok()?).max(0) as usize;
    let mut index = 4;
    index += buffer.get(index..)?.iter().position(|byte| *byte == 0)?;
    while buffer.get(index) == Some(&0) {
        index += 1;
    }
    let mut arguments = Vec::new();
    for _ in 0..argc {
        let tail = buffer.get(index..)?;
        let end = tail.iter().position(|byte| *byte == 0)?;
        if end > 0 {
            arguments.push(String::from_utf8_lossy(&tail[..end]).into_owned());
        }
        index += end + 1;
    }
    (!arguments.is_empty()).then(|| arguments.join(" "))
}

fn process_io(pid: c_int) -> (u64, u64) {
    let mut usage = [0_u64; 64];
    if unsafe { proc_pid_rusage(pid, 6, usage.as_mut_ptr().cast()) } == 0 {
        (usage[18], usage[19])
    } else {
        (0, 0)
    }
}

fn read_battery() -> Option<BatterySample> {
    unsafe {
        let info = IOPSCopyPowerSourcesInfo();
        if info.is_null() {
            return None;
        }
        let list = IOPSCopyPowerSourcesList(info);
        if list.is_null() || CFArrayGetCount(list) == 0 {
            if !list.is_null() {
                CFRelease(list);
            }
            CFRelease(info);
            return None;
        }
        let mut description: *const c_void = ptr::null();
        for index in 0..CFArrayGetCount(list) {
            let source = CFArrayGetValueAtIndex(list, index);
            let candidate = IOPSGetPowerSourceDescription(info, source);
            if candidate.is_null() {
                continue;
            }
            if description.is_null() {
                description = candidate;
            }
            if cf_dictionary_value(candidate, "Type")
                .and_then(|value| cf_string_to_string(value))
                .as_deref()
                == Some("InternalBattery")
            {
                description = candidate;
                break;
            }
        }
        if description.is_null() {
            CFRelease(list);
            CFRelease(info);
            return None;
        }
        let current = i64::from(cf_dictionary_i32(description, "Current Capacity").unwrap_or(0));
        let maximum = i64::from(cf_dictionary_i32(description, "Max Capacity").unwrap_or(100));
        let percent = battery_percent(current, maximum);
        let charging = cf_dictionary_bool(description, "Is Charging").unwrap_or(false);
        let charged = cf_dictionary_bool(description, "Is Charged").unwrap_or(false);
        let power_state = cf_dictionary_value(description, "Power Source State")
            .and_then(|value| cf_string_to_string(value));
        let minutes = if charging {
            cf_dictionary_i32(description, "Time to Full Charge")
        } else if power_state.as_deref() == Some("Battery Power") {
            cf_dictionary_i32(description, "Time to Empty")
        } else {
            None
        }
        .filter(|value| *value > 0);
        CFRelease(list);
        CFRelease(info);
        Some(BatterySample {
            percent,
            status: if charged || (percent == 100 && !charging) {
                "full"
            } else if charging {
                "charging"
            } else if power_state.as_deref() == Some("Battery Power") {
                "discharging"
            } else {
                "unknown"
            }
            .to_string(),
            watts: None,
            seconds: minutes.map(|minutes| minutes as u64 * 60),
        })
    }
}

fn battery_percent(current: i64, maximum: i64) -> u8 {
    if maximum <= 0 {
        return 0;
    }
    current
        .saturating_mul(100)
        .saturating_add(maximum / 2)
        .checked_div(maximum)
        .unwrap_or(0)
        .clamp(0, 100) as u8
}

unsafe fn cf_dictionary_i32(dictionary: *const c_void, key: &str) -> Option<i32> {
    let value = unsafe { cf_dictionary_value(dictionary, key) }?;
    if unsafe { !cf_is_type(value, CFNumberGetTypeID()) } {
        return None;
    }
    let mut result = 0_i32;
    unsafe { CFNumberGetValue(value, 3, (&mut result as *mut i32).cast()) }.then_some(result)
}

fn cf_string(value: &str) -> Option<*const c_void> {
    let value = CString::new(value).ok()?;
    let string = unsafe { CFStringCreateWithCString(ptr::null(), value.as_ptr(), 0x0800_0100) };
    (!string.is_null()).then_some(string)
}

unsafe fn cf_string_to_string(value: *const c_void) -> Option<String> {
    if unsafe { !cf_is_type(value, CFStringGetTypeID()) } {
        return None;
    }
    let mut buffer = [0_i8; 256];
    unsafe {
        CFStringGetCString(
            value,
            buffer.as_mut_ptr(),
            buffer.len() as isize,
            0x0800_0100,
        )
    }
    .then(|| {
        unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned()
    })
}

unsafe fn cf_dictionary_value(dictionary: *const c_void, key: &str) -> Option<*const c_void> {
    if unsafe { !cf_is_type(dictionary, CFDictionaryGetTypeID()) } {
        return None;
    }
    let key = CString::new(key).ok()?;
    let cf_key = unsafe { CFStringCreateWithCString(ptr::null(), key.as_ptr(), 0x0800_0100) };
    if cf_key.is_null() {
        return None;
    }
    let value = unsafe { CFDictionaryGetValue(dictionary, cf_key) };
    unsafe { CFRelease(cf_key) };
    (!value.is_null()).then_some(value)
}

unsafe fn cf_dictionary_i64(dictionary: *const c_void, key: &str) -> Option<i64> {
    let value = unsafe { cf_dictionary_value(dictionary, key) }?;
    if unsafe { !cf_is_type(value, CFNumberGetTypeID()) } {
        return None;
    }
    let mut result = 0_i64;
    unsafe { CFNumberGetValue(value, 4, (&mut result as *mut i64).cast()) }.then_some(result)
}

unsafe fn cf_dictionary_u64(dictionary: *const c_void, key: &str) -> Option<u64> {
    u64::try_from(unsafe { cf_dictionary_i64(dictionary, key) }?).ok()
}

unsafe fn cf_dictionary_bool(dictionary: *const c_void, key: &str) -> Option<bool> {
    let value = unsafe { cf_dictionary_value(dictionary, key) }?;
    unsafe { cf_is_type(value, CFBooleanGetTypeID()).then(|| CFBooleanGetValue(value)) }
}

unsafe fn cf_is_type(value: *const c_void, expected: usize) -> bool {
    !value.is_null() && unsafe { CFGetTypeID(value) == expected }
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
    buffer.truncate(size);
    while buffer.last() == Some(&0) {
        buffer.pop();
    }
    String::from_utf8(buffer).ok()
}

fn sysctl_value<T: Default>(name: &str) -> Option<T> {
    let name = CString::new(name).ok()?;
    let mut value = T::default();
    let mut size = mem::size_of::<T>();
    if unsafe {
        sysctlbyname(
            name.as_ptr(),
            (&mut value as *mut T).cast(),
            &mut size,
            ptr::null_mut(),
            0,
        )
    } == 0
        && size <= mem::size_of::<T>()
    {
        Some(value)
    } else {
        None
    }
}

fn c_array_opt<const N: usize>(value: &[c_char; N]) -> Option<String> {
    let value = c_array_string(value);
    (!value.is_empty()).then_some(value)
}

fn c_array_string<const N: usize>(value: &[c_char; N]) -> String {
    let length = value.iter().position(|byte| *byte == 0).unwrap_or(N);
    String::from_utf8_lossy(unsafe {
        std::slice::from_raw_parts(value.as_ptr().cast::<u8>(), length)
    })
    .into_owned()
}

#[derive(Clone, Default)]
struct Interface {
    received: u64,
    transmitted: u64,
    ipv4: Option<String>,
    ipv6: Option<String>,
    connected: bool,
}

#[repr(C)]
#[derive(Default)]
struct Timeval {
    seconds: i64,
    microseconds: i32,
}

#[repr(C, align(8))]
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

#[repr(C)]
#[derive(Default)]
struct XswUsage {
    total: u64,
    available: u64,
    used: u64,
    page_size: u32,
    encrypted: c_int,
}

#[repr(C)]
struct StatFs {
    block_size: u32,
    io_size: i32,
    blocks: u64,
    blocks_free: u64,
    blocks_available: u64,
    files: u64,
    files_free: u64,
    fsid: [i32; 2],
    owner: u32,
    filesystem_type: u32,
    flags: u32,
    filesystem_subtype: u32,
    filesystem: [c_char; 16],
    mountpoint: [c_char; 1024],
    mounted_from: [c_char; 1024],
    flags_ext: u32,
    reserved: [u32; 7],
}

#[repr(C)]
struct IfAddrs {
    next: *mut IfAddrs,
    name: *mut c_char,
    flags: c_uint,
    address: *mut SockAddr,
    netmask: *mut SockAddr,
    destination: *mut SockAddr,
    data: *mut c_void,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddr {
    length: u8,
    family: u8,
    data: [u8; 14],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrIn {
    length: u8,
    family: u8,
    port: u16,
    address: u32,
    zero: [u8; 8],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct SockAddrIn6 {
    length: u8,
    family: u8,
    port: u16,
    flow_info: u32,
    address: [u8; 16],
    scope_id: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct RouteMessageHeader {
    length: u16,
    version: u8,
    kind: u8,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IfData64Prefix {
    kind: u8,
    type_length: u8,
    physical: u8,
    address_length: u8,
    header_length: u8,
    receive_quota: u8,
    transmit_quota: u8,
    unused: u8,
    mtu: u32,
    metric: u32,
    baud_rate: u64,
    packets_received: u64,
    receive_errors: u64,
    packets_transmitted: u64,
    transmit_errors: u64,
    collisions: u64,
    bytes_received: u64,
    bytes_transmitted: u64,
}

#[repr(C)]
#[derive(Clone, Copy)]
struct IfMessageHeader2 {
    length: u16,
    version: u8,
    kind: u8,
    addresses: c_int,
    flags: c_int,
    index: u16,
    send_length: c_int,
    send_max_length: c_int,
    send_drops: c_int,
    timer: c_int,
    data: IfData64Prefix,
}

#[repr(C)]
#[derive(Default)]
struct ProcBsdInfo {
    flags: u32,
    status: u32,
    exit_status: u32,
    pid: u32,
    parent_pid: u32,
    uid: u32,
    gid: u32,
    real_uid: u32,
    real_gid: u32,
    saved_uid: u32,
    saved_gid: u32,
    reserved: u32,
    command: [c_char; 16],
    name: [c_char; 32],
    open_files: u32,
    process_group: u32,
    job_control_count: u32,
    terminal_device: u32,
    terminal_process_group: u32,
    nice: i32,
    start_seconds: u64,
    start_microseconds: u64,
}

#[repr(C)]
#[derive(Default)]
struct ProcBsdShortInfo {
    pid: u32,
    parent_pid: u32,
    process_group: u32,
    status: u32,
    command: [c_char; 16],
    flags: u32,
    uid: u32,
    gid: u32,
    real_uid: u32,
    real_gid: u32,
    saved_uid: u32,
    saved_gid: u32,
    reserved: u32,
}

#[repr(C)]
#[derive(Default)]
struct ProcTaskInfo {
    virtual_size: u64,
    resident_size: u64,
    total_user: u64,
    total_system: u64,
    threads_user: u64,
    threads_system: u64,
    policy: i32,
    faults: i32,
    pageins: i32,
    copy_on_write_faults: i32,
    messages_sent: i32,
    messages_received: i32,
    mach_syscalls: i32,
    unix_syscalls: i32,
    context_switches: i32,
    thread_count: i32,
    running_thread_count: i32,
    priority: i32,
}

#[repr(C)]
#[derive(Default)]
struct MachTimebaseInfo {
    numer: u32,
    denom: u32,
}

// pw_name is the first member on Darwin; no later fields are accessed.
#[repr(C)]
struct Passwd {
    name: *mut c_char,
}

// The Linux collector declares these names with Linux's ABI-specific ifaddrs
// and sockaddr layouts. Only this declaration is called on Darwin.
#[allow(clashing_extern_declarations)]
unsafe extern "C" {
    static mach_task_self_: c_uint;
    fn mach_host_self() -> c_uint;
    fn host_processor_info(
        host: c_uint,
        flavor: c_int,
        cpu_count: *mut c_uint,
        processor_info: *mut *mut c_int,
        processor_info_count: *mut c_uint,
    ) -> c_int;
    fn vm_deallocate(task: c_uint, address: usize, size: usize) -> c_int;
    fn host_statistics64(
        host: c_uint,
        flavor: c_int,
        info: *mut c_int,
        count: *mut c_uint,
    ) -> c_int;
    fn getloadavg(load_average: *mut f64, count: c_int) -> c_int;
    fn sysconf(name: c_int) -> i64;
    fn mach_timebase_info(info: *mut MachTimebaseInfo) -> c_int;
    fn sysctlbyname(
        name: *const c_char,
        old: *mut c_void,
        old_size: *mut usize,
        new: *mut c_void,
        new_size: usize,
    ) -> c_int;
    fn sysctl(
        name: *mut c_int,
        name_length: c_uint,
        old: *mut c_void,
        old_size: *mut usize,
        new: *mut c_void,
        new_size: usize,
    ) -> c_int;
    fn getmntinfo(mounts: *mut *mut StatFs, flags: c_int) -> c_int;
    fn getifaddrs(addresses: *mut *mut IfAddrs) -> c_int;
    fn freeifaddrs(addresses: *mut IfAddrs);
    fn if_indextoname(index: c_uint, name: *mut c_char) -> *mut c_char;
    fn proc_listallpids(buffer: *mut c_void, buffer_size: c_int) -> c_int;
    fn proc_pidinfo(
        pid: c_int,
        flavor: c_int,
        argument: u64,
        buffer: *mut c_void,
        buffer_size: c_int,
    ) -> c_int;
    fn proc_name(pid: c_int, buffer: *mut c_void, buffer_size: u32) -> c_int;
    fn proc_pidpath(pid: c_int, buffer: *mut c_void, buffer_size: u32) -> c_int;
    fn proc_pid_rusage(pid: c_int, flavor: c_int, buffer: *mut c_void) -> c_int;
    fn getpwuid(uid: u32) -> *mut Passwd;
}

#[link(name = "SystemConfiguration", kind = "framework")]
unsafe extern "C" {
    fn SCDynamicStoreCopyValue(store: *const c_void, key: *const c_void) -> *const c_void;
}

#[link(name = "IOKit", kind = "framework")]
unsafe extern "C" {
    fn IOServiceMatching(name: *const c_char) -> *const c_void;
    fn IOServiceGetMatchingServices(
        main_port: u32,
        matching: *const c_void,
        iterator: *mut u32,
    ) -> c_int;
    fn IOIteratorNext(iterator: u32) -> u32;
    fn IOObjectRelease(object: u32) -> c_int;
    fn IORegistryEntryGetParentEntry(entry: u32, plane: *const c_char, parent: *mut u32) -> c_int;
    fn IORegistryEntryCreateCFProperty(
        entry: u32,
        key: *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> *const c_void;
    fn IORegistryEntryCreateCFProperties(
        entry: u32,
        properties: *mut *const c_void,
        allocator: *const c_void,
        options: u32,
    ) -> c_int;
    fn IOPSCopyPowerSourcesInfo() -> *const c_void;
    fn IOPSCopyPowerSourcesList(info: *const c_void) -> *const c_void;
    fn IOPSGetPowerSourceDescription(info: *const c_void, source: *const c_void) -> *const c_void;
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(value: *const c_void);
    fn CFGetTypeID(value: *const c_void) -> usize;
    fn CFBooleanGetTypeID() -> usize;
    fn CFDictionaryGetTypeID() -> usize;
    fn CFNumberGetTypeID() -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFArrayGetCount(array: *const c_void) -> isize;
    fn CFArrayGetValueAtIndex(array: *const c_void, index: isize) -> *const c_void;
    fn CFDictionaryGetValue(dictionary: *const c_void, key: *const c_void) -> *const c_void;
    fn CFStringCreateWithCString(
        allocator: *const c_void,
        value: *const c_char,
        encoding: u32,
    ) -> *const c_void;
    fn CFStringGetCString(
        string: *const c_void,
        buffer: *mut c_char,
        buffer_size: isize,
        encoding: u32,
    ) -> bool;
    fn CFNumberGetValue(number: *const c_void, number_type: c_int, value: *mut c_void) -> bool;
    fn CFBooleanGetValue(boolean: *const c_void) -> bool;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unsigned_cpu_ticks_extend_across_the_u32_wrap() {
        assert_eq!(extend_u32_counter(0x8000_0000, 0), 0x8000_0000);
        assert_eq!(extend_u32_counter(0x20, 0xffff_fff0), 0x1_0000_0020);
        assert_eq!(extend_u32_counter(0x30, 0x1_0000_0020), 0x1_0000_0030);
    }

    #[test]
    fn route_interface_header_matches_the_darwin_64_bit_counter_prefix() {
        assert_eq!(mem::size_of::<RouteMessageHeader>(), 4);
        assert_eq!(mem::offset_of!(IfMessageHeader2, data), 32);
        assert_eq!(mem::offset_of!(IfData64Prefix, bytes_received), 64);
        assert_eq!(mem::offset_of!(IfData64Prefix, bytes_transmitted), 72);
    }

    #[test]
    fn memory_categories_do_not_mix_page_state_with_backing_type() {
        let stats = VmStatistics64 {
            internal_page_count: 100,
            wire_count: 20,
            compressor_page_count: 30,
            purgeable_count: 10,
            external_page_count: 200,
            active_count: 999,
            ..VmStatistics64::default()
        };
        assert_eq!(macos_used_pages(&stats), 140);
    }

    #[test]
    fn battery_capacity_uses_the_source_reported_maximum() {
        assert_eq!(battery_percent(2_500, 5_000), 50);
        assert_eq!(battery_percent(95, 100), 95);
        assert_eq!(battery_percent(1, 0), 0);
        assert_eq!(battery_percent(6_000, 5_000), 100);
    }

    #[test]
    fn disk_counters_use_elapsed_time_and_native_busy_time() {
        let old = super::super::DiskCounters {
            sectors_read: 1_000,
            sectors_written: 2_000,
            activity: 5_000_000_000,
            activity_valid: true,
        };
        let now = super::super::DiskCounters {
            sectors_read: 5_000,
            sectors_written: 8_000,
            activity: 5_500_000_000,
            activity_valid: true,
        };

        assert_eq!(
            macos_disk_counter_delta(old, now, 2.0),
            (2_000, 3_000, 25.0)
        );

        let unavailable = super::super::DiskCounters {
            activity_valid: false,
            ..now
        };
        assert_eq!(macos_disk_counter_delta(old, unavailable, 2.0).2, 0.0);
    }

    #[test]
    #[ignore = "requires live macOS host APIs outside the Codex sandbox"]
    fn live_collector_returns_core_system_data() {
        let config = Config::default();
        let mut collector = Collector::new(&config).unwrap();
        let first = collector.collect(&config, None).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let sample = collector.collect(&config, None).unwrap();

        assert!(!sample.cpu.name.is_empty());
        assert!(!sample.cpu.cores.is_empty());
        assert!(sample.memory.total > 0);
        assert!(sample.memory.disks.iter().any(|disk| disk.mount == "/"));
        assert!(sample.memory.disks.iter().any(|disk| disk.io_supported));
        assert!(
            collector
                .previous_disks
                .values()
                .any(|counters| counters.activity_valid)
        );
        assert!(!sample.network.interfaces.is_empty());
        let native_network = interface_counters64().unwrap();
        assert!(!native_network.is_empty());
        assert!(
            native_network
                .values()
                .any(|(received, transmitted)| received.saturating_add(*transmitted) > 0)
        );
        if let Some(primary) = primary_network_interface() {
            assert!(native_network.contains_key(&primary));
            assert_eq!(sample.network.selected, primary);
        }
        assert!(
            sample
                .processes
                .iter()
                .any(|process| process.pid == std::process::id())
        );
        assert_eq!(sample.process_count, sample.processes.len());
        let host_process_count = unsafe { proc_listallpids(ptr::null_mut(), 0) } as usize;
        assert!(sample.process_count >= host_process_count / 2);
        assert!(
            sample
                .processes
                .iter()
                .any(|process| process.user.parse::<u32>().is_err())
        );
        assert_eq!(first.cpu.cores.len(), sample.cpu.cores.len());
        #[cfg(target_arch = "aarch64")]
        {
            assert!(sample.cpu.temperature.is_some());
            assert_eq!(sample.gpus.len(), 1);
            assert!(sample.gpus[0].support.utilization);
            assert!(sample.gpus[0].support.gpu_clock);
            assert!(sample.gpus[0].gpu_clock_mhz > 0);
            assert!(sample.gpus[0].support.power_state);
            assert_eq!(sample.gpus[0].power_limit_mw, 0);
            assert!(sample.gpus[0].support.memory_total);
            assert!(sample.gpus[0].support.memory_used);
            assert!(sample.gpus[0].support.unified_memory);
            assert!(sample.gpus[0].support.encoder_sessions);
            assert!(sample.gpus[0].support.decoder_sessions);
            assert_eq!(sample.gpus[0].memory_total, sample.memory.total);
            assert!(sample.gpus[0].memory_used <= sample.gpus[0].memory_total);
            if sample.cpu.name.contains("A18") {
                assert!(sample.gpus[0].name.contains("5-core GPU"));
                assert!(sample.gpus[0].temperature_c > 0);
                assert!(sample.gpus[0].support.memory_utilization);
                assert!(sample.gpus[0].memory_utilization <= 100);
            }
        }
    }
}
