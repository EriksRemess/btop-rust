use super::*;
use std::os::unix::fs::symlink;

struct Fixture {
    root: PathBuf,
    hwmon: PathBuf,
    collector: GpuCollector,
    config: Config,
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!("btoprs-amd-{name}-{}", std::process::id()));
        let device = root.join("card0/device");
        let hwmon = device.join("hwmon/hwmon0");
        fs::create_dir_all(&hwmon).unwrap();
        fs::write(device.join("vendor"), "0x1002\n").unwrap();
        fs::write(device.join("device"), "0x150e\n").unwrap();
        symlink("/sys/bus/pci/drivers/amdgpu", device.join("driver")).unwrap();
        fs::write(hwmon.join("power1_average"), "35000000\n").unwrap();
        let mut config = Config::default();
        config.set_value("shown_gpus", "amd");
        let collector = GpuCollector {
            nvml: None,
            rsmi: None,
            amd_sysfs: discover_amd_sysfs_at(&root),
            intel: None,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            apple: None,
            shown: "amd".into(),
            next_probe: Instant::now() + Duration::from_secs(10),
        };
        Self {
            root,
            hwmon,
            collector,
            config,
        }
    }

    fn sample(&mut self) -> GpuSample {
        self.collector.collect(&self.config, false).remove(0)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn amd_sysfs_power_uses_readable_sources_and_recovers() {
    let mut fixture = Fixture::new("power");
    fs::write(fixture.hwmon.join("power1_input"), "42000000\n").unwrap();
    assert_eq!(fixture.sample().power_mw, 35000);
    for invalid in ["", "unavailable", "-1"] {
        fs::write(fixture.hwmon.join("power1_average"), invalid).unwrap();
        let sample = fixture.sample();
        assert!(sample.support.power);
        assert_eq!(sample.power_mw, 42000, "{invalid}");
    }
    fs::remove_file(fixture.hwmon.join("power1_average")).unwrap();
    assert_eq!(fixture.sample().power_mw, 42000);
    fs::write(fixture.hwmon.join("power1_input"), "unavailable").unwrap();
    assert!(!fixture.sample().support.power);
    fs::write(fixture.hwmon.join("power1_average"), "0\n").unwrap();
    let idle = fixture.sample();
    assert!(idle.support.power);
    assert_eq!(idle.power_mw, 0);
}

#[test]
fn amd_sysfs_respects_temperature_setting() {
    let mut fixture = Fixture::new("temperature");
    fs::write(fixture.hwmon.join("temp1_input"), "51000\n").unwrap();
    for enabled in [false, true, false] {
        fixture.config.set_value("check_temp", enabled.to_string());
        let sample = fixture.sample();
        assert_eq!(sample.support.temperature, enabled);
        if enabled {
            assert_eq!(sample.temperature_c, 51);
        }
    }
}

#[test]
fn amd_sysfs_clock_converts_units_before_narrowing() {
    let mut fixture = Fixture::new("clock");
    for (hz, mhz) in [(1_800_000_000_i64, 1800), (5_000_000_000, 5000), (0, 0)] {
        fs::write(fixture.hwmon.join("freq1_input"), hz.to_string()).unwrap();
        let sample = fixture.sample();
        assert!(sample.support.gpu_clock);
        assert_eq!(sample.gpu_clock_mhz, mhz);
    }
    fs::write(fixture.hwmon.join("freq1_input"), "-1").unwrap();
    assert!(!fixture.sample().support.gpu_clock);
}
