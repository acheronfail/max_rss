use std::process::{Command, Stdio};
use std::thread::sleep;
use std::time::Duration;

use nix::sys::wait::wait;

fn main() {
    let count = 10;

    let mut children = vec![];
    while children.len() < count {
        let child = Command::new("yes")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("failed to spawn yes");

        children.push(child);
    }

    sleep(Duration::from_secs(1));

    for mut child in children {
        child.kill().expect("failed to kill child");
    }

    for _ in 0..count {
        wait().expect("failed waiting for child");
    }
    wait().expect_err("got wait status after all children should have exited");
}
