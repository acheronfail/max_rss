use nix::unistd::{fork, getpid, ForkResult};

fn main() {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            println!("child:  {}", getpid());
        }
        Ok(ForkResult::Parent { child }) => {
            println!("parent: {} (of {})", getpid(), child);
        }
        Err(e) => panic!("{}", e),
    }
}
