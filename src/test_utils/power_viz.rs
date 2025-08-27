use std::collections::HashMap;
use std::time::Duration;
use plotters::prelude::*;
use plotters::style::{FontDesc, FontFamily};
use plotters::series::LineSeries;

#[derive(Debug, Default)]
pub struct PowerTimeSeries {
    timestamps: Vec<Duration>,
    package_power: Vec<f64>,
    dram_power: Vec<f64>,
    cpu_freq: Vec<f64>,
    cpu_temp: Vec<f64>,
}

impl PowerTimeSeries {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn calculate_efficiency(&self) -> Vec<f64> {
        self.package_power.iter()
            .zip(self.dram_power.iter())
            .zip(self.cpu_freq.iter())
            .map(|((pkg, dram), freq)| {
                if *freq > 0.0 {
                    (pkg + dram) / freq
                } else {
                    0.0
                }
            })
            .collect()
    }

    pub fn add_sample(&mut self, timestamp: Duration, package_power: f64, dram_power: f64, 
                     cpu_freq: f64, cpu_temp: f64) {
        self.timestamps.push(timestamp);
        self.package_power.push(package_power);
        self.dram_power.push(dram_power);
        self.cpu_freq.push(cpu_freq);
        self.cpu_temp.push(cpu_temp);
    }
}

use std::path::Path;
// use thiserror::Error;

#[derive(Debug)]
pub enum PowerVizError {
    IoError(std::io::Error),
    BackendInitError(String),
    DrawingError(String),
    NoDataError(String),
    ChartError(String),
}

type Result<T> = std::result::Result<T, PowerVizError>;

impl From<std::io::Error> for PowerVizError {
    fn from(err: std::io::Error) -> Self {
        PowerVizError::IoError(err)
    }
}

impl From<Box<dyn std::error::Error>> for PowerVizError {
    fn from(err: Box<dyn std::error::Error>) -> Self {
        PowerVizError::BackendInitError(err.to_string())
    }
}

impl<E: std::error::Error + Send + Sync> From<DrawingAreaErrorKind<E>> for PowerVizError {
    fn from(err: DrawingAreaErrorKind<E>) -> Self {
        PowerVizError::DrawingError(err.to_string())
    }
}

impl From<String> for PowerVizError {
    fn from(err: String) -> Self {
        PowerVizError::DrawingError(err)
    }
}

impl std::fmt::Display for PowerVizError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PowerVizError::IoError(err) => write!(f, "IO error: {}", err),
            PowerVizError::BackendInitError(err) => write!(f, "Backend initialization failed: {}", err),
            PowerVizError::DrawingError(err) => write!(f, "Drawing operation failed: {}", err),
            PowerVizError::NoDataError(err) => write!(f, "No data available: {}", err),
            PowerVizError::ChartError(err) => write!(f, "Chart configuration error: {}", err),
        }
    }
}

impl std::error::Error for PowerVizError {}

#[derive(Debug)]
pub struct PowerVisualizer {
    output_dir: String,
}

impl PowerVisualizer {
    pub fn new(output_dir: &str) -> Result<Self> {
        std::fs::create_dir_all(output_dir)?;
        Ok(Self {
            output_dir: output_dir.to_string(),
        })
    }

    pub fn plot_time_series(&self, data: &PowerTimeSeries, title: &str) -> Result<()> {
        if data.timestamps.is_empty() {
            return Err(PowerVizError::NoDataError("No data points available for plotting".into()));
        }
        
        let path = Path::new(&self.output_dir)
            .join(format!("power_time_series_{}.png", title.to_lowercase().replace(" ", "_")));
            
        const WIDTH: u32 = 1024;
        const HEIGHT: u32 = 768;
        
        let mut buffer = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        let root = BitMapBackend::with_buffer(&mut buffer, (WIDTH, HEIGHT))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let max_time = data.timestamps.last().unwrap_or(&Duration::from_secs(1)).as_secs_f64();
        let (min_power, max_power) = self.get_power_range(data);

        // Use a deterministic font family that Plotters bundles support for
        let _caption_font: FontDesc = (FontFamily::SansSerif, 12).into_font();
        let mut builder_binding = ChartBuilder::on(&root);
        let mut chart_builder = builder_binding
            .margin(10)
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Right, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40);

        // Headless guard: skip all text rendering when HEADLESS_PLOT=1
        let headless = std::env::var("HEADLESS_PLOT").unwrap_or_default() == "1";
        if !headless {
            chart_builder = chart_builder.caption(title, ("", 0).into_font());
        }

        let mut chart = chart_builder.build_cartesian_2d(
                0f64..max_time,
                (min_power - min_power.abs() * 0.1)..(max_power + max_power.abs() * 0.1)
            )?
            .set_secondary_coord(
                0f64..max_time,
                -5f64..105f64 // Wider range for temperature scale
            );

        if !headless {
            chart.configure_mesh()
                .x_desc("Time (seconds)")
                .y_desc("Power (Watts)")
                .axis_style(BLACK.mix(0.8).stroke_width(2))
                .draw()?;
        }

        let series = chart.draw_series(LineSeries::new(
            data.timestamps.iter().zip(&data.package_power)
                .map(|(t, p)| (t.as_secs_f64(), *p)),
            RED.mix(0.9).stroke_width(2)
        ))?;
        if !headless {
            let _ = series.label("Package Power")
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], RED.mix(0.9)));
        }

        let series2 = chart.draw_series(LineSeries::new(
            data.timestamps.iter().zip(&data.dram_power)
                .map(|(t, p)| (t.as_secs_f64(), *p)),
            BLUE.mix(0.9).stroke_width(2)
        ))?;
        if !headless {
            let _ = series2.label("DRAM Power")
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], BLUE.mix(0.9)));
        }

        let series3 = chart.draw_secondary_series(LineSeries::new(
            data.timestamps.iter().zip(&data.cpu_temp)
                .map(|(t, temp)| (t.as_secs_f64(), *temp)),
            GREEN.mix(0.8).stroke_width(2)
        ))?;
        if !headless {
            let _ = series3.label("CPU Temperature (°C)")
                .legend(|(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], GREEN.mix(0.8)));
        }

        if !headless {
            chart.configure_series_labels()
                .background_style(WHITE.mix(0.8))
                .border_style(BLACK)
                .position(SeriesLabelPosition::UpperRight)
                .draw()?;
        }

        // Ensure drawing is finalized before writing
        drop(chart);
        drop(root);
        // Persist the buffer to file
        let _ = std::fs::write(&path, &buffer);
        Ok(())


    }

    fn get_power_range(&self, data: &PowerTimeSeries) -> (f64, f64) {
        let min_power = data.package_power.iter()
            .chain(data.dram_power.iter())
            .fold(f64::MAX, |a, &b| a.min(b));
        let max_power = data.package_power.iter()
            .chain(data.dram_power.iter())
            .fold(f64::MIN, |a, &b| a.max(b));
        (min_power, max_power)
    }
    pub fn plot_power_comparison(&self, profiles: &HashMap<String, PowerTimeSeries>) -> Result<()> {
        if profiles.is_empty() {
            return Err(PowerVizError::NoDataError("No profiles available for comparison".into()));
        }

        let path = Path::new(&self.output_dir).join("power_comparison.png");

        const WIDTH: u32 = 1024;
        const HEIGHT: u32 = 768;

        let mut buffer = vec![0u8; (WIDTH * HEIGHT * 4) as usize];
        let root = BitMapBackend::with_buffer(&mut buffer, (WIDTH, HEIGHT))
            .into_drawing_area();
        root.fill(&WHITE)?;

        let max_time = profiles.values()
            .filter_map(|p| p.timestamps.last())
            .max()
            .unwrap_or(&Duration::from_secs(1))
            .as_secs_f64();

        let (min_power, max_power) = profiles.values()
            .fold((f64::MAX, f64::MIN), |(min, max), series| {
                let series_min = series.package_power.iter()
                    .chain(series.dram_power.iter())
                    .fold(f64::MAX, |a, &b| a.min(b));
                let series_max = series.package_power.iter()
                    .chain(series.dram_power.iter())
                    .fold(f64::MIN, |a, &b| a.max(b));
                (min.min(series_min), max.max(series_max))
            });

        let _caption_font: FontDesc = (FontFamily::SansSerif, 12).into_font();
        let mut builder_binding = ChartBuilder::on(&root);
        let mut chart_builder = builder_binding
            .margin(10)
            .set_label_area_size(LabelAreaPosition::Left, 60)
            .set_label_area_size(LabelAreaPosition::Bottom, 40);

        let headless = std::env::var("HEADLESS_PLOT").unwrap_or_default() == "1";
        if !headless {
            chart_builder = chart_builder.caption("Power Consumption Comparison", ("", 0).into_font());
        }

        let mut chart = chart_builder.build_cartesian_2d(0f64..max_time, min_power..max_power)?;

        if !headless {
            chart.configure_mesh()
                .x_desc("Time (seconds)")
                .y_desc("Total Power (Watts)")
                .draw()?;
        }

        for (i, (name, data)) in profiles.iter().enumerate() {
            let color = Palette99::pick(i).mix(0.9);
            let ds = chart.draw_series(LineSeries::new(
                data.timestamps.iter().zip(data.package_power.iter().zip(&data.dram_power))
                    .map(|(t, (p, d))| (t.as_secs_f64(), p + d)),
                color.stroke_width(2)
            ))?;
            if !headless {
                let _ = ds.label(name)
                    .legend(move |(x, y)| PathElement::new(vec![(x, y), (x + 20, y)], color));
            }
        }

        if !headless {
            chart.configure_series_labels()
                .background_style(WHITE.mix(0.8))
                .border_style(BLACK)
                .position(SeriesLabelPosition::UpperRight)
                .draw()?;
        }

        // Ensure chart/root dropped before writing buffer
        drop(chart);
        drop(root);
        // Persist buffer to file
        std::fs::write(&path, &buffer)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;
    
    #[test]
    fn test_power_timeseries() {
        let mut series = PowerTimeSeries::new();
        
        // Test empty series
        assert_eq!(series.calculate_efficiency(), Vec::<f64>::new());
        
        // Test adding and retrieving samples
        series.add_sample(
            Duration::from_secs(1), 
            50.0, // package power
            10.0, // dram power
            2400.0, // cpu freq
            65.0 // cpu temp
        );
        
        assert_eq!(series.timestamps.len(), 1);
        assert_eq!(series.package_power.len(), 1);
        assert_eq!(series.dram_power.len(), 1);
        assert_eq!(series.cpu_freq.len(), 1);
        assert_eq!(series.cpu_temp.len(), 1);
        
        // Test efficiency calculation
        let efficiency = series.calculate_efficiency();
        assert_eq!(efficiency.len(), 1);
        assert!((efficiency[0] - (60.0 / 2400.0)).abs() < 1e-10);
    }

    #[test]
    fn test_power_visualizer() -> Result<()> {
        std::env::set_var("HEADLESS_PLOT", "1");
        let test_dir = "target/test_power_viz";
        
        // Create test data
        let mut data = PowerTimeSeries::new();
        for i in 0..100 {
            data.add_sample(
                Duration::from_secs(i),
                50.0 + (i as f64 / 10.0).sin() * 5.0,
                10.0 + (i as f64 / 15.0).cos() * 2.0,
                2400.0 + (i as f64 / 20.0).sin() * 200.0,
                65.0 + (i as f64 / 25.0).cos() * 5.0
            );
        }
        
        // Test empty data case
        let viz = PowerVisualizer::new(test_dir)?;
        let empty_data = PowerTimeSeries::new();
        assert!(viz.plot_time_series(&empty_data, "Empty Profile").is_err());
        
        // Test valid data plotting
        viz.plot_time_series(&data, "Test Power Profile")?;
        assert!(Path::new(test_dir).join("power_time_series_test_power_profile.png").exists());
        
        // Test power comparison plotting
        let mut profiles = HashMap::new();
        profiles.insert("Profile 1".to_string(), data);
        viz.plot_power_comparison(&profiles)?;
        assert!(Path::new(test_dir).join("power_comparison.png").exists());
        
        // Cleanup test directory
        fs::remove_dir_all(test_dir)?;
        Ok(())
    }

    #[test]
    fn test_plotting_range() -> Result<()> {
        std::env::set_var("HEADLESS_PLOT", "1");
        let test_dir = "target/test_power_viz_range";
        let viz = PowerVisualizer::new(test_dir)?;
        
        // Create test data with wide range of values
        let mut data = PowerTimeSeries::new();
        for i in 0..100 {
            data.add_sample(
                Duration::from_secs(i),
                500.0 + (i as f64 / 10.0).sin() * 50.0,  // Higher power values
                100.0 + (i as f64 / 15.0).cos() * 20.0,  // Higher DRAM values
                4800.0 + (i as f64 / 20.0).sin() * 400.0, // Higher frequency
                85.0 + (i as f64 / 25.0).cos() * 10.0     // Higher temperature
            );
        }
        
        // Test plotting with wide range
        viz.plot_time_series(&data, "Wide Range Test")?;
        assert!(Path::new(test_dir).join("power_time_series_wide_range_test.png").exists());
        
        // Cleanup
        fs::remove_dir_all(test_dir)?;
        Ok(())
    }
}