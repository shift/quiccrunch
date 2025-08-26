    // Add power metrics to report
    writeln!(report, "\n## Power Consumption Analysis\n")?;
    writeln!(report, "| Implementation | Avg Package Power (W) | Avg DRAM Power (W) | Energy/Message (mJ) | Energy/MB (J) |")?;
    writeln!(report, "|----------------|---------------------|------------------|-------------------|--------------|")?;

    for impl_name in &self.comparison_matrix.implementations {
        let power_stats = &self.power_profiles[impl_name];
        writeln!(report, "| {} | {:.2} | {:.2} | {:.3} | {:.3} |",
            impl_name,
            power_stats.avg_package_power,
            power_stats.avg_dram_power,
            power_stats.energy_per_message * 1000.0, // Convert to mJ
            power_stats.energy_per_byte * 1024.0 * 1024.0, // Convert to J/MB
        )?;
    }

    // Add energy efficiency analysis
    writeln!(report, "\n### Energy Efficiency Analysis\n")?;
    
    // Find most energy efficient implementation
    let most_efficient = self.power_profiles.iter()
        .min_by(|a, b| {
            (a.1.energy_per_byte).partial_cmp(&b.1.energy_per_byte).unwrap()
        })
        .unwrap();

    writeln!(report, "Most energy efficient implementation: **{}**\n", most_efficient.0)?;
    writeln!(report, "Energy efficiency characteristics:")?;
    writeln!(report, "- {:.2} Joules per MB transferred", most_efficient.1.energy_per_byte * 1024.0 * 1024.0)?;
    writeln!(report, "- {:.2} mJ per message", most_efficient.1.energy_per_message * 1000.0)?;
    writeln!(report, "- Average power consumption: {:.2}W (Package: {:.2}W, DRAM: {:.2}W)",
        most_efficient.1.avg_package_power + most_efficient.1.avg_dram_power,
        most_efficient.1.avg_package_power,
        most_efficient.1.avg_dram_power,
    )?;

    // Generate power efficiency plot
    self.plot_power_efficiency()?;
    writeln!(report, "\n![Power Efficiency Comparison](power_efficiency.png)\n")?;