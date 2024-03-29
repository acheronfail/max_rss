use std::fs;
use std::io::ErrorKind;
use std::process::{Command, Stdio};

use serde_json::Value;

fn cmd(bin: &str, args: &[&str]) -> String {
    let output = Command::new(bin)
        .args(args)
        .stderr(Stdio::piped())
        .stdout(Stdio::null())
        .output()
        .expect(&format!("failed to run command: {} {:?}", bin, args));

    String::from_utf8_lossy(&output.stderr).to_string()
}

fn run(example_name: &str) -> Value {
    let bin = format!(
        "./target/{}/examples/{}",
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        },
        example_name
    );

    let out = format!("{}.json", example_name);
    match fs::remove_file(&out) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => panic!("{}", e),
    }

    let stderr = cmd(
        "cargo",
        &[
            "run",
            "--",
            "--return-result",
            "--debug",
            "--output",
            &out,
            &bin,
        ],
    );

    let text = fs::read_to_string(&out).expect("failed to read output");
    let json = serde_json::from_str::<Value>(&text).expect("failed to parse JSON");

    eprintln!("{}", stderr);
    dbg!(json)
}

#[test]
fn print() {
    let json = run("print");
    assert_eq!(json["total_pids"], 1);
    assert_eq!(json["total_reads"], 1);
}

#[test]
fn fork() {
    let json = run("fork");
    assert_eq!(json["total_pids"], 2);
    assert_eq!(json["total_reads"], 1);
}

#[test]
fn double_fork() {
    let json = run("double_fork");
    assert_eq!(json["total_pids"], 4);
    assert_eq!(json["total_reads"], 2);
}

#[test]
fn threads() {
    let json = run("threads");
    assert_eq!(json["total_pids"], 11);
    assert_eq!(json["total_reads"], 1);
}

#[test]
fn fork_threads() {
    let json = run("fork_threads");
    assert_eq!(json["total_pids"], 12);
    assert_eq!(json["total_reads"], 2);
}

#[test]
fn kill_threads() {
    let json = run("kill_threads");
    assert_eq!(json["total_pids"], 11);
    assert_eq!(json["total_reads"], 1);
}

#[test]
fn tracee_exit_0() {
    let json = run("true");
    assert_eq!(json["exit_code"], 0);
    assert_eq!(json["total_pids"], 1);
    assert_eq!(json["total_reads"], 1);
}

#[test]
fn tracee_exit_1() {
    let json = run("false");
    assert_eq!(json["exit_code"], 1);
    assert_eq!(json["total_pids"], 1);
    assert_eq!(json["total_reads"], 1);
}
