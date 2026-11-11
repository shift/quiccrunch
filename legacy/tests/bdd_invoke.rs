use std::process::Command;

#[test]
fn run_bdd_example() {
    let status = Command::new(env!("CARGO"))
        .args([
            "run",
            "--no-default-features",
            "--features",
            "backend-tquic,visualization",
            "--example",
            "bdd",
        ])
        .status()
        .expect("failed to spawn bdd example");
    assert!(status.success(), "BDD example exited with non-zero status");
}
