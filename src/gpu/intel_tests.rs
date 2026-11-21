use super::*;
use std::io::Write;
use std::os::fd::IntoRawFd;

fn counter(previous: u64, name: &str) -> (PmuCounter, fs::File) {
    let path = std::env::temp_dir().join(format!("btoprs-intel-{name}-{}", std::process::id()));
    let writer = fs::File::create(&path).unwrap();
    let reader = fs::File::open(&path).unwrap();
    fs::remove_file(path).unwrap();
    (
        PmuCounter {
            fd: reader.into_raw_fd(),
            previous: Some(previous),
            scale: 1.0,
        },
        writer,
    )
}

fn fixture(name: &str) -> (IntelGpu, [fs::File; 3]) {
    let (busy, busy_writer) = counter(1_000_000_000, &format!("{name}-busy"));
    let (frequency, frequency_writer) = counter(1000, &format!("{name}-frequency"));
    let (energy, energy_writer) = counter(1000, &format!("{name}-energy"));
    (
        IntelGpu {
            #[cfg(target_os = "linux")]
            drm: None,
            name: "test GPU".into(),
            busy: vec![busy],
            frequency: Some(frequency),
            sysfs_frequency: None,
            energy: Some(energy),
            last_sample: Instant::now(),
            max_power_mw: 0,
        },
        [busy_writer, frequency_writer, energy_writer],
    )
}

fn sample(collector: &mut IntelGpu, writers: &mut [fs::File; 3], values: [u64; 3]) -> GpuSample {
    for (writer, value) in writers.iter_mut().zip(values) {
        writer.write_all(&value.to_ne_bytes()).unwrap();
    }
    collector.last_sample = Instant::now() - Duration::from_secs(1);
    collector.collect()
}

fn assert_available(sample: &GpuSample, expected: bool) {
    assert_eq!(sample.support.utilization, expected, "utilization");
    assert_eq!(sample.support.gpu_clock, expected, "frequency");
    assert_eq!(sample.support.power, expected, "power");
}

#[test]
fn intel_counter_failures_and_resets_require_a_new_baseline() {
    let (mut collector, mut writers) = fixture("recovery");
    assert_available(
        &sample(&mut collector, &mut writers, [2_000_000_000, 2000, 2000]),
        true,
    );
    assert_available(&collector.collect(), false);
    assert_available(
        &sample(&mut collector, &mut writers, [3_000_000_000, 3000, 3000]),
        false,
    );
    let recovered = sample(&mut collector, &mut writers, [3_500_000_000, 3500, 3500]);
    assert_available(&recovered, true);
    assert_eq!(recovered.utilization, 50);
    assert!((495..=500).contains(&recovered.gpu_clock_mhz));
    assert!((495_000..=500_000).contains(&recovered.power_mw));
    assert_available(
        &sample(&mut collector, &mut writers, [100_000_000, 100, 100]),
        false,
    );
    let idle = sample(&mut collector, &mut writers, [100_000_000, 100, 100]);
    assert_available(&idle, true);
    assert_eq!(idle.utilization, 0);
    assert_eq!(idle.gpu_clock_mhz, 0);
    assert_eq!(idle.power_mw, 0);
}

#[test]
fn intel_failed_frequency_counter_falls_back_to_current_sysfs_reading() {
    let path = std::env::temp_dir().join(format!("btoprs-intel-fallback-{}", std::process::id()));
    let cleanup = DropGuard::new(path, |path| {
        let _ = fs::remove_file(path);
    });
    fs::write(&*cleanup, "350\n").unwrap();
    let (mut collector, _writers) = fixture("fallback");
    collector.sysfs_frequency = Some((*cleanup).clone());
    let fallback = collector.collect();
    assert!(fallback.support.gpu_clock);
    assert_eq!(fallback.gpu_clock_mhz, 350);
    for invalid in ["", "unavailable", "-1"] {
        fs::write(&*cleanup, invalid).unwrap();
        assert!(!collector.collect().support.gpu_clock, "{invalid}");
    }
    fs::write(&*cleanup, "400\n").unwrap();
    let recovered = collector.collect();
    assert!(recovered.support.gpu_clock);
    assert_eq!(recovered.gpu_clock_mhz, 400);
}
