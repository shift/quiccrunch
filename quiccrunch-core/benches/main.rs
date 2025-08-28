use criterion::{criterion_group, criterion_main, Criterion};
use quiccrunch::benches::{
    comparison::run_performance_matrix,
    report::TestReport,
};
use tokio::runtime::Runtime;

fn bench_all_implementations(c: &mut Criterion) {
    let rt = Runtime::new().unwrap();
    
    println!("Starting comprehensive performance comparison...");
    println!("This may take several minutes to complete.");
    
    // Run the full test matrix
    let results = rt.block_on(async {
        run_performance_matrix().await.unwrap()
    });
    
    // Generate the report
    results.generate().unwrap();
    
    println!("\nBenchmark complete! Reports generated in target/benchmark/:");
    println!("- comparison_report.md - Detailed analysis");
    println!("- throughput_comparison.png - Throughput graphs");
    println!("- latency_distribution.png - Latency analysis");
    println!("- concurrency_scaling.png - Scaling characteristics");
}

criterion_group! {
    name = benches;
    config = Criterion::default()
        .sample_size(10)
        .measurement_time(Duration::from_secs(30))
        .warm_up_time(Duration::from_secs(5));
    targets = bench_all_implementations
}
criterion_main!(benches);