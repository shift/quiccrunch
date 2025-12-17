use std::process::{Command, Stdio};

fn run_example(args: &[&str], features: &[&str]) -> (i32, String, String) {
    let mut cmd = Command::new("cargo");
    cmd.arg("run");
    if !features.is_empty() {
        cmd.arg("--no-default-features");
        cmd.arg("--features");
        cmd.arg(features.join(","));
    }
    cmd.arg("--example");
    cmd.arg(args[0]);
    cmd.arg("--");
    for a in &args[1..] {
        cmd.arg(a);
    }
    cmd.env("RUST_LOG", "");
    cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    let out = cmd.output().expect("spawn example");
    let code = out.status.code().unwrap_or(-1);
    (
        code,
        String::from_utf8_lossy(&out.stdout).to_string(),
        String::from_utf8_lossy(&out.stderr).to_string(),
    )
}


