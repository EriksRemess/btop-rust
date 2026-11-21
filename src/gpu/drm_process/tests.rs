use super::*;
use std::os::unix::fs::symlink;

const PCI: &str = "0000:03:00.0";

fn fixture(name: &str) -> std::mem::DropGuard<PathBuf, impl FnOnce(PathBuf)> {
    let root = std::env::temp_dir().join(format!("btoprs-drm-{name}-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    std::mem::DropGuard::new(root, |root| fs::remove_dir_all(root).unwrap())
}

fn fdinfo(driver: &str, id: u64, fields: &str) -> String {
    format!("drm-driver:\t{driver}\ndrm-pdev:\t{PCI}\ndrm-client-id:\t{id}\n{fields}")
}

#[test]
fn attributes_resident_bytes_to_regions_and_preserves_unknowns() {
    let fields = "drm-resident-vram: 1025\ndrm-memory-vram: 1 KiB\ndrm-memory-gtt: 2 MiB\ndrm-total-vram: 64 MiB";
    let text = fdinfo("amdgpu", 3, fields);
    let client = parse(&text, Driver::Amd, PCI).unwrap();
    assert_eq!(client.dedicated, Some(1025));
    assert_eq!(client.bytes, Some(2 * 1024 * 1024 + 1025));
    assert!(parse(&text, Driver::Xe, PCI).is_none());
    assert!(parse(&text, Driver::Amd, "0000:03:00.1").is_none());
    assert!(parse(&text.replace("drm-client-id", "other"), Driver::Amd, PCI).is_none());

    let text = fdinfo(
        "i915",
        6,
        "drm-resident-system0: 5 KiB\ndrm-total-system0: 10 MiB",
    );
    let client = parse(&text, Driver::I915, PCI).unwrap();
    assert_eq!(client.bytes, Some(5120));
    assert_eq!(client.dedicated, None);
    let text = fdinfo(
        "i915",
        6,
        "drm-resident-local0: 1 MiB\ndrm-resident-local1: 2 MiB\ndrm-resident-system0: 123 bogus",
    );
    let client = parse(&text, Driver::I915, PCI).unwrap();
    assert_eq!(client.dedicated, Some(3 * 1024 * 1024));
    assert_eq!(client.bytes, None);
    assert_eq!(bytes("18446744073709551615 MiB"), None);
    assert!(
        parse(
            &fdinfo("xe", 1, "drm-cycles-rcs: 0\ndrm-total-cycles-rcs: 0"),
            Driver::Xe,
            PCI
        )
        .unwrap()
        .bytes
        .is_none()
    );
    assert!(parse(&fdinfo("xe", 1, "ino: 5"), Driver::Xe, PCI).is_none());
    assert!(parse(&fdinfo("xe", 1, "drm-cycles-rcs: 1"), Driver::Xe, PCI).is_none());
}

fn measured(driver: Driver, fields: &str, at: Instant) -> Measurement {
    let name = match driver {
        Driver::Amd => "amdgpu",
        Driver::I915 => "i915",
        Driver::Xe => "xe",
    };
    let client = parse(&fdinfo(name, 1, fields), driver, PCI).unwrap();
    Measurement {
        at,
        engines: client.engines,
    }
}

#[test]
fn amd_zero_engine_times_can_be_omitted_and_appear_on_first_use() {
    let now = Instant::now();
    let sample = |seconds, engines| {
        measured(
            Driver::Amd,
            &format!("drm-memory-vram: 64 KiB\n{engines}"),
            now + Duration::from_secs(seconds),
        )
    };
    let idle = sample(0, "");
    let mut still_idle = sample(1, "");
    assert_eq!(still_idle.usage(&idle, Driver::Amd), Some(0.0));
    let mut active = sample(2, "drm-engine-gfx: 250000000 ns");
    assert_eq!(active.usage(&still_idle, Driver::Amd), Some(25.0));
    let mut compute = sample(
        3,
        "drm-engine-gfx: 250000000 ns\ndrm-engine-compute: 300000000 ns",
    );
    assert_eq!(compute.usage(&active, Driver::Amd), Some(30.0));

    // Memory-only Intel drivers do not define absent engine times as zero.
    let intel = |seconds| {
        measured(
            Driver::I915,
            "drm-resident-system0: 64 KiB",
            now + Duration::from_secs(seconds),
        )
    };
    assert_eq!(intel(1).usage(&intel(0), Driver::I915), None);
}

#[test]
fn intel_engine_usage_accounts_for_capacity_and_counter_regressions() {
    let now = Instant::now();
    let i915 = |ns: u64, elapsed| {
        measured(
            Driver::I915,
            &format!("drm-engine-render: {ns} ns\ndrm-engine-capacity-render: 2"),
            now + Duration::from_secs(elapsed),
        )
    };
    let prev = i915(2_000_000_000, 0);
    let mut next = i915(3_000_000_000, 1);
    assert_eq!(next.usage(&prev, Driver::I915), Some(50.0));
    let mut low = i915(2_500_000_000, 2);
    assert_eq!(low.usage(&next, Driver::I915), None);
    assert_eq!(i915(3_100_000_000, 3).usage(&low, Driver::I915), Some(5.0));

    let xe = |rcs, ccs, ticks| {
        measured(
            Driver::Xe,
            &format!(
                "drm-cycles-rcs: {rcs}\ndrm-total-cycles-rcs: {ticks}\ndrm-cycles-ccs: {ccs}\ndrm-total-cycles-ccs: {ticks}\ndrm-engine-capacity-ccs: 4\ndrm-cycles-vcs: 10"
            ),
            now,
        )
    };
    let prior = xe(100, 100, 1000);
    let mut next = xe(120, 140, 1100);
    assert_eq!(next.usage(&prior, Driver::Xe), Some(30.0));
    assert_eq!(xe(120, 140, 1100).usage(&next, Driver::Xe), None);
    let mut missing = measured(
        Driver::Xe,
        "drm-cycles-rcs: 130\ndrm-resident-system: 0",
        now,
    );
    assert_eq!(missing.usage(&next, Driver::Xe), None);
}

fn device(root: &Path, driver: &str, pci: &str, node: &str) -> Device {
    let path = root.join("sys").join(pci);
    let dev = root.join("dev").join(pci);
    fs::create_dir_all(path.join("drm/renderD128")).unwrap();
    let dev_number = match node {
        "/dev/null" => "1:3",
        "/dev/zero" => "1:5",
        _ => unreachable!(),
    };
    fs::write(path.join("drm/renderD128/dev"), dev_number).unwrap();
    fs::create_dir_all(&dev).unwrap();
    symlink(
        format!("/sys/bus/pci/drivers/{driver}"),
        path.join("driver"),
    )
    .unwrap();
    symlink(node, dev.join("renderD128")).unwrap();
    Device::discover_at(&path, &dev, &root.join("topology"), false).unwrap()
}

fn fd(root: &Path, pid: u32, number: u16, node: &str, info: &str) {
    let dir = root.join(pid.to_string());
    fs::create_dir_all(dir.join("fd")).unwrap();
    fs::create_dir_all(dir.join("fdinfo")).unwrap();
    let path = dir.join("fd").join(number.to_string());
    if !path.exists() {
        symlink(node, path).unwrap();
    }
    fs::write(dir.join("fdinfo").join(number.to_string()), info).unwrap();
}

fn process(pid: u32) -> ProcessSample {
    ProcessSample {
        pid,
        start_time: Some(1),
        ..Default::default()
    }
}

#[test]
fn proc_sweep_maps_devices_deduplicates_fds_and_resets_reused_pids() {
    let root = fixture("sweep");
    let mut processes = [process(47), process(83), process(99)];
    fs::create_dir_all(root.join("83/fd")).unwrap();
    let mut amd = device(&root, "amdgpu", PCI, "/dev/null");
    let mut intel = device(&root, "i915", "0000:00:02.0", "/dev/zero");
    let update_amd = |ns: u64, compute: u64| {
        let info = fdinfo(
            "amdgpu",
            9,
            &format!("drm-memory-vram: 2 KiB\ndrm-engine-gfx: {ns} ns"),
        );
        for number in [3, 4] {
            fd(&root, 47, number, "/dev/null", &info);
        }
        fd(
            &root,
            47,
            5,
            "/dev/null",
            &fdinfo(
                "amdgpu",
                10,
                &format!("drm-memory-vram: 0\ndrm-engine-compute: {compute} ns"),
            ),
        );
    };
    update_amd(2_000_000_000, 0);
    fd(
        &root,
        47,
        7,
        "/dev/zero",
        &fdinfo(
            "i915",
            9,
            "drm-resident-local0: 4 KiB\ndrm-resident-system0: 1 KiB\ndrm-engine-render: 150 ns",
        )
        .replace(PCI, "0000:00:02.0"),
    );
    let now = Instant::now();
    let mut sample = |processes: &mut [ProcessSample], requested, seconds| {
        let mut gpus = vec![GpuSample::default(); 3];
        collect(
            &mut [(2, &mut intel), (1, &mut amd)],
            &mut gpus,
            processes,
            requested,
            &root,
            now + Duration::from_secs(seconds),
        );
        let starts: Vec<_> = processes.iter().map(|p| (p.pid, p.start_time)).collect();
        crate::process_gpu::attach(processes, &gpus, |pid| {
            starts.iter().find(|(id, _)| *id == pid)?.1
        });
        gpus
    };
    let hidden = sample(&mut processes, false, 0);
    assert!(hidden[1].support.processes && hidden[2].support.processes);
    assert!(hidden.iter().all(|g| g.process_usage.is_none()));
    sample(&mut processes, true, 1);
    assert_eq!(processes[0].gpu.utilization, None);
    update_amd(2_404_000_000, 4_000_000);
    sample(&mut processes, true, 2);
    let usage = &processes[0].gpu;
    assert_eq!(usage.bytes, Some(7 * 1024));
    assert_eq!(usage.dedicated_bytes, Some(6 * 1024));
    assert_eq!(usage.utilization, Some(41));
    assert_eq!(
        usage.devices.iter().map(|d| d.index).collect::<Vec<_>>(),
        [1, 2]
    );
    assert_eq!(processes[1].gpu.bytes, Some(0));
    assert_eq!(processes[2].gpu.bytes, None);
    processes[0].start_time = Some(2);
    sample(&mut processes, true, 3);
    assert_eq!(processes[0].gpu.utilization, None);
    sample(&mut processes, true, 4);
    assert_eq!(processes[0].gpu.utilization, Some(0));
    sample(&mut processes, false, 5);
    sample(&mut processes, true, 6);
    assert_eq!(processes[0].gpu.utilization, None);

    fs::remove_file(root.join("47/fdinfo/5")).unwrap();
    sample(&mut processes, true, 7);
    assert_eq!(processes[0].gpu.bytes, None);
    assert_eq!(processes[0].gpu.devices[1].bytes, Some(5120));
}

#[test]
fn fdinfo_identifies_clients_when_the_render_node_is_hidden_or_remapped() {
    let root = fixture("mount");
    let pci = root.join(PCI);
    let render = pci.join("drm/renderD128");
    fs::create_dir_all(&render).unwrap();
    symlink("drivers/i915", pci.join("driver")).unwrap();
    fs::write(render.join("dev"), "1:3\n").unwrap(); // /dev/null in the fixture
    let dev_dri = root.join("dri");
    let discover = || Device::discover_at(&pci, &dev_dri, &root.join("topology"), false).unwrap();
    let mut device = discover();
    let info = fdinfo("i915", 3, "drm-resident-system0: 4 KiB");
    fd(&root, 7, 4, "/dev/null", &info);
    let mut samples = [GpuSample::default()];
    collect(
        &mut [(0, &mut device)],
        &mut samples,
        &[process(7)],
        true,
        &root,
        Instant::now(),
    );
    assert_eq!(
        samples[0].process_usage.as_ref().unwrap().processes[&7],
        Some(4096)
    );

    fs::create_dir(&dev_dri).unwrap();
    symlink("/dev/zero", dev_dri.join("renderD128")).unwrap();
    let mut device = discover();
    // A same-driver fd with no PCI field must still match the actual device number.
    let info = info
        .lines()
        .filter(|line| !line.starts_with("drm-pdev:"))
        .collect::<Vec<_>>()
        .join("\n");
    fd(&root, 8, 5, "/dev/zero", &info);
    collect(
        &mut [(0, &mut device)],
        &mut samples,
        &[process(7), process(8)],
        true,
        &root,
        Instant::now(),
    );
    assert_eq!(
        samples[0].process_usage.as_ref().unwrap().processes,
        HashMap::from([(7, Some(4096))])
    );
}

#[test]
fn capability_probes_retry_without_exposing_an_unsupported_panel() {
    let root = fixture("probe");
    let mut device = device(&root, "xe", PCI, "/dev/zero");
    let now = Instant::now();
    let mut sample = |seconds, requested| {
        let mut gpus = [GpuSample::default()];
        collect(
            &mut [(0, &mut device)],
            &mut gpus,
            &[process(7)],
            requested,
            &root,
            now + Duration::from_secs(seconds),
        );
        gpus.into_iter().next().unwrap()
    };
    assert!(!sample(0, false).support.processes);
    fd(
        &root,
        7,
        3,
        "/dev/zero",
        &fdinfo("xe", 1, "drm-resident-gtt: 0"),
    );
    assert!(!sample(9, false).support.processes);
    assert!(sample(10, false).support.processes);
    fs::remove_file(root.join("7/fd/3")).unwrap();
    let idle = sample(11, true).process_usage.unwrap();
    assert!(idle.available && idle.processes.is_empty());
}

#[test]
fn amd_kfd_occupancy_uses_matching_render_minor_and_compute_unit_count() {
    let root = fixture("kfd");
    let node = root.join("2");
    fs::create_dir(&node).unwrap();
    fs::write(node.join("gpu_id"), "735\n").unwrap();
    fs::write(
        node.join("properties"),
        "drm_render_minor 129\nsimd_count 128\nsimd_per_cu 4\n",
    )
    .unwrap();
    assert!(Kfd::discover(128, &root).is_none());
    let kfd = Kfd::discover(129, &root).unwrap();
    assert_eq!(kfd.occupancy(&root, 42), None);
    let stats = root.join("42/stats_735");
    fs::create_dir_all(&stats).unwrap();
    fs::write(stats.join("cu_occupancy"), "8\n").unwrap();
    assert_eq!(kfd.occupancy(&root, 42), Some(25));

    let mut device = device(&root, "amdgpu", PCI, "/dev/null");
    device.kfd = Some(kfd);
    for (drm_usage, expected) in [
        (None, None),
        (Some(20.0), Some(45)),
        (Some(85.0), Some(100)),
    ] {
        let scan = Scan {
            index: 0,
            device: &mut device,
            snapshot: Snapshot::default(),
            utilization: HashMap::from([(42, drm_usage)]),
            current: HashMap::new(),
        };
        assert_eq!(scan.finish(1, &root).utilization.unwrap()[&42], expected);
    }
}

#[test]
#[ignore = "requires an accessible AMD or Intel GPU with Linux DRM client accounting"]
fn collects_live_drm_processes() {
    use crate::{config::Config, gpu::GpuCollector};
    let mut config = Config::default();
    config.set_value("shown_gpus", "amd intel");
    let mut collector = GpuCollector::new(&config);
    // Hold real render clients open so the test also works on an idle system.
    let clients: Vec<_> = fs::read_dir("/sys/class/drm")
        .unwrap()
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            name.to_str()?
                .strip_prefix("renderD")?
                .parse::<u32>()
                .ok()?;
            let device = fs::canonicalize(entry.path().join("device")).ok()?;
            let driver = fs::read_link(device.join("driver")).ok()?;
            let driver = Driver::parse(driver.file_name()?.to_str()?)?;
            let file = fs::File::open(Path::new("/dev/dri").join(name)).ok()?;
            let text =
                fs::read_to_string(format!("/proc/self/fdinfo/{}", file.as_raw_fd())).ok()?;
            parse(&text, driver, device.file_name()?.to_str()?)?;
            Some(file)
        })
        .collect();
    assert!(
        !clients.is_empty(),
        "no AMD or Intel render client exposes readable process counters"
    );
    let me = process(std::process::id());
    let gpus = collector.collect(&config, true, std::slice::from_ref(&me));
    let detected = gpus
        .iter()
        .filter(|gpu| {
            gpu.support.processes
                && gpu
                    .process_usage
                    .as_ref()
                    .is_some_and(|data| data.processes.contains_key(&me.pid))
        })
        .count();
    assert_eq!(
        detected,
        clients.len(),
        "opened render clients must appear in their GPU snapshots"
    );
}

#[test]
fn process_only_card_is_published_after_the_first_successful_probe() {
    use crate::{
        config::Config,
        gpu::{GpuCollector, IntelGpu},
    };
    let root = fixture("no-telemetry");
    let card = root.join("card0");
    let device = card.join("device");
    fs::create_dir_all(device.join("drm/renderD999")).unwrap();
    fs::write(device.join("drm/renderD999/dev"), "1:3").unwrap();
    fs::write(device.join("vendor"), "0x8086").unwrap();
    symlink("drivers/i915", device.join("driver")).unwrap();
    let mut cfg = Config::default();
    cfg.set_value("shown_gpus", "intel");
    let mut collector = GpuCollector::new(&cfg);
    collector.intel = IntelGpu::load_at(&root, &root.join("pmu"));
    assert_eq!(collector.intel.len(), 1);
    assert!(
        collector.collect(&cfg, false, &[]).is_empty(),
        "driver identification alone provides no displayable telemetry"
    );
    fd(
        &root,
        7,
        3,
        "/dev/null",
        "drm-driver: i915\ndrm-client-id: 42\ndrm-resident-system0: 4096",
    );
    collect(
        &mut [(0, collector.intel[0].drm.as_mut().unwrap())],
        &mut [GpuSample::default()],
        &[process(7)],
        false,
        &root,
        Instant::now() + Duration::from_secs(11),
    );
    let sample = collector.collect(&cfg, false, &[]);
    assert_eq!(sample.len(), 1);
    assert!(sample[0].support.processes);
}
