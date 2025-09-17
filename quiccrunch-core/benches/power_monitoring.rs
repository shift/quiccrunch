use criterion::{criterion_group, criterion_main, Criterion};
use quiccrunch_core::power::PowerMonitor;
use quiccrunch_core::test_utils::power_viz::{PowerTimeSeries, PowerVisualizer};
use std::collections::HashMap;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn collect_power_profile(duration: Duration) -> PowerTimeSeries {
    let mut monitor = PowerMonitor::new().unwrap();
    let mut series = PowerTimeSeries::new();
    let start = Instant::now();

    while start.elapsed() < duration {
        if let (Ok(package), Ok(dram), Ok(freq), Ok(temp)) = (
            monitor.get_package_power(),
            monitor.get_dram_power(),
            monitor.get_cpu_frequency(),
            monitor.get_cpu_temperature(),
        ) {
            series.add_sample(start.elapsed(), package, dram, freq, temp);
        }
        thread::sleep(Duration::from_millis(100));
    }

    series
}

fn power_monitoring_benchmark(c: &mut Criterion) {
    let mut group = c.benchmark_group("Power Monitoring");
    let viz = PowerVisualizer::new("target/benchmark");

    // Collect power profiles under different load conditions
    let mut profiles = HashMap::new();

    // Idle profile
    let idle_profile = collect_power_profile(Duration::from_secs(10));
    viz.plot_time_series(&idle_profile, "Idle System").unwrap();
    profiles.insert("Idle".to_string(), idle_profile);

    // CPU stress profile
    let cpu_stress_profile = {
        let handle = thread::spawn(|| {
            // Simple CPU stress
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                let _ = (0..1000000).sum::<i32>();
            }
        });
        let profile = collect_power_profile(Duration::from_secs(10));
        handle.join().unwrap();
        profile
    };
    viz.plot_time_series(&cpu_stress_profile, "CPU Stress")
        .unwrap();
    profiles.insert("CPU Stress".to_string(), cpu_stress_profile);

    // Memory stress profile
    let memory_stress_profile = {
        let handle = thread::spawn(|| {
            // Allocate and access memory
            let mut data = Vec::with_capacity(1_000_000);
            let start = Instant::now();
            while start.elapsed() < Duration::from_secs(10) {
                data.extend(0..1000);
                data.clear();
            }
        });
        let profile = collect_power_profile(Duration::from_secs(10));
        handle.join().unwrap();
        profile
    };
    viz.plot_time_series(&memory_stress_profile, "Memory Stress")
        .unwrap();
    profiles.insert("Memory Stress".to_string(), memory_stress_profile);

    // Test different CPU governors
    let governors = vec!["powersave", "performance", "schedutil"];

    for governor in governors {
        // Set governor (requires root)
        Command::new("sudo")
            .args(&["cpupower", "frequency-set", "-g", governor])
            .output()
            .expect("Failed to set CPU governor");

        thread::sleep(Duration::from_secs(1)); // Let system stabilize

        let governor_profile = collect_power_profile(Duration::from_secs(10));
        viz.plot_time_series(&governor_profile, &format!("Governor: {}", governor))
            .unwrap();
        profiles.insert(format!("Governor: {}", governor), governor_profile);
    }

    // Generate comparison plot
    viz.plot_power_comparison(&profiles).unwrap();

    // Validate power efficiency metrics
    let validate_efficiency = |profile: &PowerTimeSeries, name: &str| {
        let efficiency = profile.calculate_efficiency();

        // Basic validation
        assert!(
            !efficiency.is_empty(),
            "Efficiency vector should not be empty for {}",
            name
        );

        // Check for invalid values
        for (i, eff) in efficiency.iter().enumerate() {
            assert!(
                *eff >= 0.0,
                "Negative efficiency at index {} for {}: {}",
                i,
                name,
                eff
            );
            assert!(
                *eff < 1000.0,
                "Unreasonably high efficiency at index {} for {}: {}",
                i,
                name,
                eff
            );
        }

        // Validate efficiency trends
        let avg_efficiency: f64 = efficiency.iter().sum::<f64>() / efficiency.len() as f64;
        println!(
            "Average efficiency for {}: {:.2} watts/GHz",
            name, avg_efficiency
        );

        // Return for comparison
        (avg_efficiency, efficiency)
    };

    // Validate each profile
    for (name, profile) in &profiles {
        validate_efficiency(profile, name);
    }

    // Compare efficiencies between different workloads
    let (idle_eff, _) = validate_efficiency(&profiles["Idle"], "Idle");
    let (cpu_stress_eff, _) = validate_efficiency(&profiles["CPU Stress"], "CPU Stress");
    let (mem_stress_eff, _) = validate_efficiency(&profiles["Memory Stress"], "Memory Stress");

    // Basic sanity checks
    assert!(
        cpu_stress_eff > idle_eff,
        "CPU stress should show higher power/frequency ratio than idle"
    );
    assert!(
        mem_stress_eff > idle_eff,
        "Memory stress should show higher power/frequency ratio than idle"
    );

    // Benchmark the monitoring itself
    let mut monitor = PowerMonitor::new().unwrap();
    group.bench_function("measure_power", |b| {
        b.iter(|| {
            monitor.update_metrics().unwrap();
            thread::sleep(Duration::from_millis(100));
        })
    });

    group.finish();
}

criterion_group!(benches, power_monitoring_benchmark);
criterion_main!(benches);
