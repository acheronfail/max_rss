use std::fs;
use std::io::ErrorKind;
use std::process::{Command, Stdio};

use serde_json::Value;

fn cmd(bin: &str, args: &[&str]) {
    assert!(Command::new(bin)
        .args(args)
        .stderr(Stdio::null())
        .stdout(Stdio::null())
        .status()
        .expect("failed to build example")
        .success());
}

fn run(example_name: &str) -> Value {
    let bin = format!("./target/debug/examples/{}", example_name);
    let out = format!("{}.json", example_name);

    cmd("cargo", &["build", "--example", example_name]);

    match fs::remove_file(&out) {
        Ok(_) => {}
        Err(e) if e.kind() == ErrorKind::NotFound => {}
        Err(e) => panic!("{}", e),
    }

    cmd("cargo", &["run", "--", "--output", &out, &bin]);

    let text = fs::read_to_string(&out).expect("failed to read output");
    let json = serde_json::from_str::<Value>(&text).expect("failed to parse JSON");

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
    let json = run("double-fork");
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
    let json = run("fork-threads");
    assert_eq!(json["total_pids"], 12);
    assert_eq!(json["total_reads"], 2);
}
