use std::{ffi::CString, fs};

use nix::{
    sys::{
        ptrace::{self, Event, Options},
        wait::{waitpid, WaitStatus, wait},
    },
    unistd::{execvp, fork, ForkResult, Pid},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            // let argv: Vec<CString> = ["bash", "-c", "(sleep 1; echo 1) & (sleep 2; echo 2)"]
            let argv: Vec<CString> = ["bash", "-c", "sleep 1; echo 1"]
                .into_iter()
                .map(|s| CString::new(s).unwrap())
                .collect();

            ptrace::traceme()?;
            execvp(&argv[0], &argv).expect_err("failed to execvp");

            Ok(())
        }
        Ok(ForkResult::Parent { child }) => {
            println!("child: {:?}", child);

            // NOTE: child should be stopped after `ptrace::traceme()`, on its first instruction
            let _ = waitpid(child, None)?;
            ptrace::setoptions(
                child,
                Options::PTRACE_O_TRACEEXIT
                    | Options::PTRACE_O_TRACEFORK
                    | Options::PTRACE_O_TRACEVFORK
                    | Options::PTRACE_O_TRACECLONE,
            )?;
            ptrace::cont(child, None)?;

            // now our loop to handle statuses
            loop {
                match dbg!(waitpid(child, None)?) {
                    WaitStatus::Exited(pid, code) => {
                        println!("child {} exited with {}", pid, code);
                        break;
                    }
                    WaitStatus::PtraceEvent(pid, _, value) => {
                        // TODO: extract rss and add sum it for final result
                        if value == Event::PTRACE_EVENT_EXIT as i32 {
                            let smaps_rollup =
                                fs::read_to_string(format!("/proc/{}/smaps_rollup", child))?;
                            let line = smaps_rollup.lines().find(|line| line.starts_with("Rss:"));
                            println!("RSS EXIT: {} -> {}", child, line.unwrap());
                        }

                        if value == Event::PTRACE_EVENT_FORK as i32 {
                            let new_pid = ptrace::getevent(pid)?;
                            let new_pid = Pid::from_raw(new_pid as i32);

                            // FIXME: this hangs, because the newly forked process is now getting
                            // traced
                            // TODO: check if we are still tracing the parent, too?
                            // TODO: setup tokio or something to manage each child
                            println!("fork: {}", new_pid);
                            let _ = waitpid(new_pid, None)?;
                            ptrace::cont(new_pid, None)?;
                        }

                        ptrace::cont(pid, None)?;
                    }
                    WaitStatus::Stopped(pid, _) => {
                        ptrace::cont(pid, None)?;
                    }
                    other => {
                        ptrace::cont(child, None)?;
                        dbg!(other);
                    }
                }
            }

            Ok(())
        }
        Err(e) => panic!("failed to fork: {}", e),
    }
}
