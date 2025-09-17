use plotters::prelude::*;
use std::path::Path;

pub fn generate_efficiency_plots(
    metrics: &[EfficiencyMetrics],
    output_dir: &Path,
) -> std::io::Result<()> {
    plot_throughput_vs_power(metrics, &output_dir.join("throughput_power.png"))?;
    plot_efficiency_ratios(metrics, &output_dir.join("efficiency_ratios.png"))?;
    plot_performance_matrix(metrics, &output_dir.join("performance_matrix.png"))?;
    Ok(())
}

fn plot_throughput_vs_power(
    metrics: &[EfficiencyMetrics],
    output_path: &Path,
) -> std::io::Result<()> {
    let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_throughput = metrics.iter().map(|m| m.bandwidth_gbps).fold(0.0, f64::max);
    let max_power = metrics
        .iter()
        .map(|m| m.power_profile.avg_package_power + m.power_profile.avg_dram_power)
        .fold(0.0, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Throughput vs Power Consumption", ("sans-serif", 30))
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..max_power, 0.0..max_throughput)?;

    chart
        .configure_mesh()
        .x_desc("Power (Watts)")
        .y_desc("Throughput (Gbps)")
        .draw()?;

    // Plot efficiency frontier
    let frontier_points: Vec<(f64, f64)> = metrics
        .iter()
        .map(|m| {
            (
                m.power_profile.avg_package_power + m.power_profile.avg_dram_power,
                m.bandwidth_gbps,
            )
        })
        .collect();

    chart.draw_series(LineSeries::new(frontier_points, &BLUE))?;

    // Add points for each measurement with performance/watt as color intensity
    let max_perf_per_watt = metrics
        .iter()
        .map(|m| m.performance_per_watt)
        .fold(0.0, f64::max);

    for m in metrics {
        let color_intensity = (m.performance_per_watt / max_perf_per_watt * 255.0) as u8;
        chart.draw_series(PointSeries::of_element(
            vec![(
                m.power_profile.avg_package_power + m.power_profile.avg_dram_power,
                m.bandwidth_gbps,
            )],
            5,
            &RGBColor(0, 0, color_intensity),
            &|coord, size, style| {
                EmptyElement::at(coord) + Circle::new((0, 0), size, style.filled())
            },
        ))?;
    }

    Ok(())
}

fn plot_efficiency_ratios(
    metrics: &[EfficiencyMetrics],
    output_path: &Path,
) -> std::io::Result<()> {
    let root = BitMapBackend::new(output_path, (800, 600)).into_drawing_area();
    root.fill(&WHITE)?;

    let max_bits_per_joule = metrics.iter().map(|m| m.bits_per_joule).fold(0.0, f64::max);

    let max_msgs_per_joule = metrics
        .iter()
        .map(|m| m.messages_per_joule)
        .fold(0.0, f64::max);

    let mut chart = ChartBuilder::on(&root)
        .caption("Energy Efficiency Ratios", ("sans-serif", 30))
        .margin(10)
        .x_label_area_size(40)
        .y_label_area_size(60)
        .build_cartesian_2d(0.0..max_msgs_per_joule, 0.0..max_bits_per_joule)?;

    chart
        .configure_mesh()
        .x_desc("Messages per Joule")
        .y_desc("Bits per Joule")
        .draw()?;

    // Plot points with latency as color
    let min_latency = metrics
        .iter()
        .map(|m| m.latency_us)
        .fold(f64::INFINITY, f64::min);

    let max_latency = metrics.iter().map(|m| m.latency_us).fold(0.0, f64::max);

    for m in metrics {
        let latency_ratio = (m.latency_us - min_latency) / (max_latency - min_latency);
        let color = RGBColor(
            (255.0 * latency_ratio) as u8,
            (255.0 * (1.0 - latency_ratio)) as u8,
            0,
        );

        chart.draw_series(PointSeries::of_element(
            vec![(m.messages_per_joule, m.bits_per_joule)],
            5,
            &color,
            &|coord, size, style| {
                EmptyElement::at(coord) + Circle::new((0, 0), size, style.filled())
            },
        ))?;
    }

    Ok(())
}

fn plot_performance_matrix(
    metrics: &[EfficiencyMetrics],
    output_path: &Path,
) -> std::io::Result<()> {
    let root = BitMapBackend::new(output_path, (1000, 1000)).into_drawing_area();
    root.fill(&WHITE)?;

    let cell_size = 200;
    let metrics_count = 5; // Number of metrics to show in matrix

    let max_values = [
        metrics.iter().map(|m| m.bandwidth_gbps).fold(0.0, f64::max),
        metrics
            .iter()
            .map(|m| m.messages_per_second)
            .fold(0.0, f64::max),
        metrics
            .iter()
            .map(|m| m.performance_per_watt)
            .fold(0.0, f64::max),
        metrics.iter().map(|m| m.bits_per_joule).fold(0.0, f64::max),
        metrics
            .iter()
            .map(|m| m.efficiency_score)
            .fold(0.0, f64::max),
    ];

    let labels = [
        "Bandwidth",
        "Messages/sec",
        "Perf/Watt",
        "Bits/Joule",
        "Efficiency",
    ];

    for i in 0..metrics_count {
        for j in 0..metrics_count {
            if i != j {
                let area = root.clone().margin(
                    (i * cell_size) as i32,
                    (j * cell_size) as i32,
                    ((metrics_count - i - 1) * cell_size) as i32,
                    ((metrics_count - j - 1) * cell_size) as i32,
                );

                let mut chart = ChartBuilder::on(&area)
                    .caption(
                        format!("{} vs {}", labels[i], labels[j]),
                        ("sans-serif", 12),
                    )
                    .build_cartesian_2d(0.0..max_values[i], 0.0..max_values[j])?;

                chart.configure_mesh().draw()?;

                // Extract values for the current metrics pair
                let points: Vec<(f64, f64)> = metrics
                    .iter()
                    .map(|m| {
                        let x = match i {
                            0 => m.bandwidth_gbps,
                            1 => m.messages_per_second,
                            2 => m.performance_per_watt,
                            3 => m.bits_per_joule,
                            4 => m.efficiency_score,
                            _ => 0.0,
                        };
                        let y = match j {
                            0 => m.bandwidth_gbps,
                            1 => m.messages_per_second,
                            2 => m.performance_per_watt,
                            3 => m.bits_per_joule,
                            4 => m.efficiency_score,
                            _ => 0.0,
                        };
                        (x, y)
                    })
                    .collect();

                chart.draw_series(PointSeries::of_element(
                    points,
                    3,
                    &BLUE,
                    &|coord, size, style| {
                        EmptyElement::at(coord) + Circle::new((0, 0), size, style.filled())
                    },
                ))?;
            }
        }
    }

    Ok(())
}
