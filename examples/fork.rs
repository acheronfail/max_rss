use nix::unistd::{fork, getpid, ForkResult};

fn print(msg: impl AsRef<str>) {
    let pid = getpid().as_raw();
    let msg = msg.as_ref();
    println!("\x1b[0;31m[{pid}]: {msg}\x1b[0m")
}

fn main() {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            print(format!("child"));
        }
        Ok(ForkResult::Parent { child }) => {
            print(format!("parent of {}", child));
        }
        Err(e) => panic!("{}", e),
    }
}
