use std::path::Path;
use plotters::prelude::*;
use plotters::series::LineSeries;
use super::scaling::ScalingTestResult;

fn plot_error_to_io_error<E: std::fmt::Display>(err: E) -> std::io::Error {
    std::io::Error::new(std::io::ErrorKind::Other, err.to_string())
}

pub fn plot_efficiency_curves(results: &[ScalingTestResult], output_dir: &Path) -> std::io::Result<()> {
    let path = output_dir.join("efficiency_curves.svg");
    
    // Initialize backend with improved quality settings
    let root = SVGBackend::new(&path, (1024, 768))
        .into_drawing_area();
    root.fill(&WHITE.mix(1.0)).map_err(plot_error_to_io_error)?;
    
    // Configure drawing area for better quality
    // Calculate ranges with safe handling of empty or invalid data
    let max_freq = results.iter()
        .filter_map(|r| r.config.frequency_mhz)
        .max()
        .unwrap_or(0) as f64;
        
    let max_perf = results.iter()
        .map(|r| r.performance.bandwidth_gbps)
        .fold(0.0, f64::max);
        
    let max_power = results.iter()
        .map(|r| r.power.avg_package_power + r.power.avg_dram_power)
        .fold(0.0, f64::max);
        
    let max_efficiency = results.iter()
        .map(|r| r.efficiency.efficiency_score)
        .fold(0.0, f64::max);

    // Create chart with improved styling
    let mut chart = ChartBuilder::on(&root)
        .caption(
            "Efficiency Analysis",
            TextStyle::from(FontDesc::new(FontFamily::SansSerif, 30.0, FontStyle::Normal))
                .color(&BLACK.mix(0.8))
        )
        .margin(10)
        .set_label_area_size(LabelAreaPosition::Left, 60)
        .set_label_area_size(LabelAreaPosition::Bottom, 50)
        .build_cartesian_2d(
            0f64..max_freq * 1.05, // Add 5% padding
            0f64..max_perf.max(max_power).max(max_efficiency) * 1.05
        ).map_err(plot_error_to_io_error)?;

    // Configure mesh with improved styling
    chart.configure_mesh()
        .disable_mesh() // Disable default mesh
        .x_desc("Frequency (MHz)")
        .y_desc("Value")
        .axis_style(BLACK.mix(0.8).stroke_width(2))
        .label_style(TextStyle::from(
            FontDesc::new(FontFamily::SansSerif, 14.0, FontStyle::Normal)
        ).color(&BLACK.mix(0.8)))
        .draw()
        .map_err(plot_error_to_io_error)?;

    // Plot performance with improved styling
    chart.draw_series(LineSeries::new(
        results.iter()
            .map(|r| (r.config.frequency_mhz.unwrap_or(0) as f64, r.performance.bandwidth_gbps)),
        BLUE.mix(0.8).stroke_width(2)
    )).map_err(plot_error_to_io_error)?.label("Performance (Gbps)")
    .legend(|(x, y)| {
        PathElement::new(
            vec![(x, y), (x + 20, y)],
            BLUE.mix(0.8).stroke_width(2)
        )
    });

    // Plot power with improved styling
    chart.draw_series(LineSeries::new(
        results.iter()
            .map(|r| (
                r.config.frequency_mhz.unwrap_or(0) as f64,
                r.power.avg_package_power + r.power.avg_dram_power
            )),
        RED.mix(0.8).stroke_width(2)
    )).map_err(plot_error_to_io_error)?.label("Power (W)")
    .legend(|(x, y)| {
        PathElement::new(
            vec![(x, y), (x + 20, y)],
            RED.mix(0.8).stroke_width(2)
        )
    });

    // Plot efficiency score with improved styling
    chart.draw_series(LineSeries::new(
        results.iter()
            .map(|r| (r.config.frequency_mhz.unwrap_or(0) as f64, r.efficiency.efficiency_score)),
        GREEN.mix(0.8).stroke_width(2)
    )).map_err(plot_error_to_io_error)?.label("Efficiency Score")
    .legend(|(x, y)| {
        PathElement::new(
            vec![(x, y), (x + 20, y)],
            GREEN.mix(0.8).stroke_width(2)
        )
    });

    // Configure legend with improved styling
    chart.configure_series_labels()
        .background_style(WHITE.mix(0.9))
        .border_style(BLACK.mix(0.8).stroke_width(1))
        .position(SeriesLabelPosition::UpperRight)
        .margin(10)
        .label_font(TextStyle::from(
            FontDesc::new(FontFamily::SansSerif, 12.0, FontStyle::Normal)
        ).color(&BLACK.mix(0.8)))
        .draw()
        .map_err(plot_error_to_io_error)?;

    // Ensure proper buffer handling and cleanup
    root.present().map_err(plot_error_to_io_error)?;
    
    Ok(())
}