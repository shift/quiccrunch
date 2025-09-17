#![cfg(feature = "visualization")]

// This test requires sudo privileges and hardware access; exclude from default suite.
// Opt-in by running: `cargo test --features scaling-tests --test scaling_benchmark`
#[cfg(feature = "scaling-tests")]
#[test]
fn run_frequency_scaling() {
    // If not root or required tools are missing, skip gracefully
    if nix::unistd::Uid::effective().as_raw() != 0 {
        eprintln!("skipping: requires root privileges (sudo)");
        return;
    }
    for tool in ["cpupower", "sensors", "dmidecode"] {
        if which::which(tool).is_err() {
            eprintln!("skipping: required tool '{}' not found in PATH", tool);
            return;
        }
    }

    // Try to create runtime
    let rt = Runtime::new().expect("Failed to create tokio runtime");

    println!("Starting comprehensive frequency scaling tests...");

    // Try to create FrequencyScaling
    let mut scaling = rt.block_on(async {
        match FrequencyScaling::new().await {
            Ok(s) => s,
            Err(e) => {
                eprintln!("Failed to initialize frequency scaling: {}", e);
                panic!("Failed to initialize frequency scaling: {}", e);
            }
        }
    });

    // Run scaling tests
    let results = rt.block_on(async {
        match scaling.run_scaling_tests().await {
            Ok(r) => r,
            Err(e) => {
                eprintln!("Failed to run scaling tests: {}", e);
                panic!("Failed to run scaling tests: {}", e);
            }
        }
    });

    // Generate report in target directory
    let output_dir = Path::new("target")
        .join("benchmark")
        .join("frequency_scaling_report");
    std::fs::create_dir_all(&output_dir).expect("Failed to create output directory");

    // Save raw results as JSON
    let json = serde_json::to_string_pretty(&results).expect("Failed to serialize results");
    std::fs::write(output_dir.join("raw_results.json"), json)
        .expect("Failed to write results file");

    // Generate plots
    plotting::plot_efficiency_curves(&results, &output_dir)
        .expect("Failed to generate efficiency curves");

    println!("Report generated at: {}", output_dir.display());
}
