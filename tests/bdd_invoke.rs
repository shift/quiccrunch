use std::process::Command;

#[test]
fn run_bdd_example() {
    // Force quinn backend for BDD until tquic is implemented
    let status = Command::new(env!("CARGO")).args(["run", "--no-default-features", "--features", "backend-quinn,visualization", "--example", "bdd"]).status().expect("failed to spawn bdd example");
    assert!(status.success(), "BDD example exited with non-zero status");
}
