    fn write_benchmark_report(&self, results_dir: &Path) -> std::io::Result<()> {
        // Create results directory with timestamp
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S");
        let report_dir = results_dir.join(format!("benchmark_{}", timestamp));
        std::fs::create_dir_all(&report_dir)?;

        // Write system profile
        let system_profile = SystemProfile::new()?;
        let system_report = system_profile.generate_system_report();
        std::fs::write(
            report_dir.join("system_profile.json"),
            system_report,
        )?;

        // Write benchmark results
        let results_json = serde_json::to_string_pretty(&BenchmarkResults {
            system_profile,
            efficiency_metrics: self.efficiency_metrics.clone(),
            power_profiles: self.power_profiles.clone(),
            throughput_stats: self.throughput_stats.clone(),
            test_configs: self.test_configs.clone(),
        })?;
        std::fs::write(
            report_dir.join("benchmark_results.json"),
            results_json,
        )?;

        // Generate Markdown report
        let mut report = std::fs::File::create(report_dir.join("report.md"))?;
        
        writeln!(report, "# QuicCrunch Benchmark Report\n")?;
        writeln!(report, "Generated: {}\n", timestamp)?;
        
        // System Information
        writeln!(report, "## System Information\n")?;
        writeln!(report, "### CPU")?;
        writeln!(report, "- Model: {}", system_profile.cpu.model_name)?;
        writeln!(report, "- Architecture: {}", system_profile.cpu.microarchitecture)?;
        writeln!(report, "- Cores: {} physical, {} logical", 
            system_profile.cpu.physical_cores,
            system_profile.cpu.logical_cores)?;
        writeln!(report, "- Base/Max Frequency: {}/{} MHz",
            system_profile.cpu.base_frequency_mhz,
            system_profile.cpu.max_frequency_mhz)?;
        writeln!(report, "- Cache:")?;
        writeln!(report, "  - L1d: {} KB", system_profile.cpu.cache_info.l1d_size_kb)?;
        writeln!(report, "  - L1i: {} KB", system_profile.cpu.cache_info.l1i_size_kb)?;
        writeln!(report, "  - L2: {} KB", system_profile.cpu.cache_info.l2_size_kb)?;
        writeln!(report, "  - L3: {} KB", system_profile.cpu.cache_info.l3_size_kb)?;
        
        writeln!(report, "\n### Memory")?;
        writeln!(report, "- Total: {} GB", system_profile.memory.total_memory_mb / 1024)?;
        writeln!(report, "- Type: {}", system_profile.memory.memory_type)?;
        writeln!(report, "- Speed: {} MHz", system_profile.memory.speed_mhz)?;
        writeln!(report, "- Channels: {}", system_profile.memory.channels)?;
        if let Some(bw) = system_profile.memory.max_bandwidth {
            writeln!(report, "- Max Bandwidth: {:.1} GB/s", bw)?;
        }
        
        writeln!(report, "\n### Power Configuration")?;
        writeln!(report, "- Package TDP: {:.1}W", system_profile.power_limits.package_tdp)?;
        writeln!(report, "- PL1: {:.1}W", system_profile.power_limits.current_pl1)?;
        writeln!(report, "- PL2: {:.1}W", system_profile.power_limits.current_pl2)?;
        writeln!(report, "- Tau: {:.2}s", system_profile.power_limits.tau)?;
        
        writeln!(report, "\n### Operating System")?;
        writeln!(report, "- OS: {} {}", system_profile.os_info.name, system_profile.os_info.version)?;
        writeln!(report, "- Kernel: {}", system_profile.os_info.kernel)?;
        writeln!(report, "- Scheduler: {}", system_profile.os_info.scheduler)?;
        writeln!(report, "- Power Governor: {}", system_profile.os_info.power_governor)?;
        
        // Performance Results
        writeln!(report, "\n## Performance Results\n")?;
        
        // Generate plots
        generate_efficiency_plots(&self.efficiency_metrics, &report_dir)?;
        
        writeln!(report, "### Performance Summary")?;
        for metric in &self.efficiency_metrics {
            writeln!(report, "\n#### {} Implementation", metric.implementation)?;
            writeln!(report, "```")?;
            metric.print_summary()?;
            writeln!(report, "```")?;
        }
        
        // Temperature and Power Analysis
        writeln!(report, "\n## System Behavior\n")?;
        
        writeln!(report, "### Temperature Profile")?;
        writeln!(report, "```")?;
        writeln!(report, "CPU Package Temperatures:")?;
        self.print_temperature_statistics(&mut report, 
            &system_profile.temperatures.cpu_package)?;
        
        writeln!(report, "\nCore Temperatures:")?;
        for (i, core_temps) in system_profile.temperatures.cpu_cores.iter().enumerate() {
            writeln!(report, "Core {}: ", i)?;
            self.print_temperature_statistics(&mut report, core_temps)?;
        }
        writeln!(report, "```")?;
        
        writeln!(report, "\n### Cooling Performance")?;
        writeln!(report, "```")?;
        writeln!(report, "CPU Fan: {:?} RPM (avg)", 
            system_profile.cooling.cpu_fan_rpm.iter()
                .map(|r| r.value)
                .sum::<f64>() / system_profile.cooling.cpu_fan_rpm.len() as f64)?;
        
        for (i, fan) in system_profile.cooling.case_fans_rpm.iter().enumerate() {
            writeln!(report, "Case Fan {}: {:?} RPM (avg)",
                i + 1,
                fan.iter()
                    .map(|r| r.value)
                    .sum::<f64>() / fan.len() as f64)?;
        }
        
        if let Some(pump) = &system_profile.cooling.pump_rpm {
            writeln!(report, "Pump Speed: {:?} RPM (avg)",
                pump.iter()
                    .map(|r| r.value)
                    .sum::<f64>() / pump.len() as f64)?;
        }
        writeln!(report, "```")?;
        
        // Plots
        writeln!(report, "\n## Performance Plots\n")?;
        writeln!(report, "![Throughput vs Power](throughput_power.png)\n")?;
        writeln!(report, "![Efficiency Ratios](efficiency_ratios.png)\n")?;
        writeln!(report, "![Performance Matrix](performance_matrix.png)\n")?;
        
        // Temperature plot
        plot_temperature_profile(&system_profile.temperatures, &report_dir)?;
        writeln!(report, "![Temperature Profile](temperature_profile.png)\n")?;
        
        Ok(())
    }

    fn print_temperature_statistics(&self, report: &mut File, readings: &[SensorReading]) -> std::io::Result<()> {
        if readings.is_empty() {
            return Ok(());
        }
        
        let values: Vec<f64> = readings.iter().map(|r| r.value).collect();
        let min = values.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = values.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        
        writeln!(report, "  Min: {:.1}°C", min)?;
        writeln!(report, "  Max: {:.1}°C", max)?;
        writeln!(report, "  Avg: {:.1}°C", avg)?;
        Ok(())
    }
}