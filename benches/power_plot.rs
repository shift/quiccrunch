    fn plot_power_efficiency(&self) -> Result<(), Box<dyn std::error::Error>> {
        let root = BitMapBackend::new("target/benchmark/power_efficiency.png", (800, 600))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let mut chart = ChartBuilder::on(&root)
            .caption("Power Efficiency vs Throughput", ("sans-serif", 30))
            .margin(10)
            .x_label_area_size(40)
            .y_label_area_size(60)
            .build_cartesian_2d(
                0f64..1f64,  // Normalized throughput
                0f64..1f64,  // Normalized power
            )?;

        chart.configure_mesh()
            .x_desc("Normalized Throughput")
            .y_desc("Normalized Power")
            .draw()?;

        // Find max values for normalization
        let max_throughput = self.summary.by_implementation.values()
            .map(|s| s.avg_throughput)
            .fold(0.0, f64::max);

        let max_power = self.power_profiles.values()
            .map(|p| p.avg_package_power + p.avg_dram_power)
            .fold(0.0, f64::max);

        // Plot efficiency curves
        for (impl_name, stats) in &self.summary.by_implementation {
            let power_stats = &self.power_profiles[impl_name];
            
            // Create points along the efficiency curve
            let points: Vec<(f64, f64)> = (0..=10).map(|i| {
                let load = i as f64 / 10.0;
                let norm_throughput = (stats.avg_throughput * load) / max_throughput;
                let norm_power = ((power_stats.avg_package_power + power_stats.avg_dram_power) * load) / max_power;
                (norm_throughput, norm_power)
            }).collect();

            chart.draw_series(LineSeries::new(
                points,
                &Palette99::pick(
                    self.comparison_matrix.implementations
                        .iter()
                        .position(|n| n == impl_name)
                        .unwrap()
                ),
            ))?.label(impl_name);
        }

        // Add diagonal line representing linear power scaling
        chart.draw_series(LineSeries::new(
            vec![(0.0, 0.0), (1.0, 1.0)],
            &BLACK.mix(0.3),
        ))?.label("Linear");

        chart.configure_series_labels()
            .background_style(&WHITE.mix(0.8))
            .border_style(&BLACK)
            .position(SeriesLabelPosition::UpperLeft)
            .draw()?;

        Ok(())
    }