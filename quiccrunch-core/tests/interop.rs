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

#[test]
fn interop_quinn_client_tquic_server_echo() {
    // Start tquic server
    let (_code, out, err) = run_example(
        &["interop_server", "--addr", "127.0.0.1:0", "--duration", "5"],
        &["backend-tquic"],
    );
    assert!(err.is_empty(), "server stderr: {}", err);
    let bound_line = out
        .lines()
        .find(|l| l.starts_with("BOUND "))
        .expect("BOUND line");
    let addr = bound_line.split_once(' ').unwrap().1.to_string();

    // Quinn client
    let (code, _out, err) = run_example(
        &[
            "interop_client",
            "--addr",
            &addr,
            "--streams",
            "1",
            "--size",
            "1024",
            "--duration",
            "3",
        ],
        &["backend-quinn"],
    );
    assert_eq!(code, 0, "client failed: {}", err);
}

#[test]
fn interop_tquic_client_quinn_server_echo() {
    // Start quinn server
    let (_code, out, err) = run_example(
        &["interop_server", "--addr", "127.0.0.1:0", "--duration", "5"],
        &["backend-quinn"],
    );
    assert!(err.is_empty(), "server stderr: {}", err);
    let bound_line = out
        .lines()
        .find(|l| l.starts_with("BOUND "))
        .expect("BOUND line");
    let addr = bound_line.split_once(' ').unwrap().1.to_string();

    // tquic client
    let (code, _out, err) = run_example(
        &[
            "interop_client",
            "--addr",
            &addr,
            "--streams",
            "1",
            "--size",
            "1024",
            "--duration",
            "3",
        ],
        &["backend-tquic"],
    );
    assert_eq!(code, 0, "client failed: {}", err);
}
