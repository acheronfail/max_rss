use nix::unistd::{fork, getpid, ForkResult};

fn print(depth: usize, msg: impl AsRef<str>) {
    let pad = "  ".repeat(depth);
    let pid = getpid().as_raw();
    let msg = msg.as_ref();
    println!("\x1b[0;31m{pad}[{pid}]: {msg}\x1b[0m")
}

fn main() {
    let mut depth = 0;
    match unsafe { fork() } {
        Ok(ForkResult::Parent { child }) => {
            print(depth, format!("parent of {}", child));
            match unsafe { fork() } {
                Ok(ForkResult::Parent { child }) => {
                    print(depth, format!("parent of {}", child));
                }
                Ok(ForkResult::Child) => {
                    depth += 1;
                    print(depth, "child");
                }
                Err(e) => panic!("{}", e),
            }
        }
        Ok(ForkResult::Child) => {
            depth += 1;
            print(depth, "child");
            match unsafe { fork() } {
                Ok(ForkResult::Parent { child }) => {
                    print(depth, format!("parent of {}", child));
                }
                Ok(ForkResult::Child) => {
                    depth += 1;
                    print(depth, "child");
                }
                Err(e) => panic!("{}", e),
            }
        }

        Err(e) => panic!("{}", e),
    }
}
