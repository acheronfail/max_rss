use std::hint::black_box;
use std::thread;

use nix::unistd::{fork, getpid, ForkResult};

fn print(msg: impl AsRef<str>) {
    let pid = getpid().as_raw();
    let msg = msg.as_ref();
    println!("\x1b[0;31m[{pid}]: {msg}\x1b[0m")
}

fn main() {
    let count = 10;
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let mut handles = vec![];
            for i in 0..count {
                handles.push(thread::spawn(move || {
                    let vec = vec![i; 1024_usize.pow(2)];
                    black_box(vec[i]);
                    print(format!("thread: {}", i));
                }));
            }

            for handle in handles.drain(..) {
                handle.join().expect("thread failed");
            }
        }
        Ok(ForkResult::Parent { child }) => print(format!(
            "I'm the parent of {} and the proud grantparent of {}!",
            child, count
        )),
        Err(e) => panic!("{}", e),
    }
}
