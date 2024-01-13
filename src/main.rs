use std::{env, ffi::CString, fs, thread, time::Duration};

use anyhow::Result;
use nix::{
    sys::{
        ptrace::{self, Event, Options},
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, fork, ForkResult, Pid},
};

fn get_rss(pid: Pid) -> Result<u64> {
    let path = format!("/proc/{}/smaps_rollup", pid);
    let smaps_rollup = fs::read_to_string(path)?;
    let line = smaps_rollup
        .lines()
        .find(|x| x.starts_with("Rss:"))
        .expect("failed to find Rss line");
    let kb_str = line
        .split_ascii_whitespace()
        .nth(1)
        .expect("failed to find rss value");
    let kb = kb_str.parse::<u64>().expect("failed to parse rss value");

    Ok(kb * 1024)
}

fn main() -> Result<()> {
    let bin = env::args().nth(1).expect("please pass bin name");
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let argv: Vec<CString> = [format!("./target/debug/{bin}")]
                .into_iter()
                .map(|s| CString::new(s).unwrap())
                .collect();

            ptrace::traceme()?;
            execvp(&argv[0], &argv).expect_err("failed to execvp");

            Ok(())
        }
        Ok(ForkResult::Parent { child }) => {
            println!("::: pid of tracee: {:?}", child);

            let options = Options::PTRACE_O_TRACEEXIT
                | Options::PTRACE_O_TRACEFORK
                | Options::PTRACE_O_TRACEVFORK
                | Options::PTRACE_O_TRACECLONE;

            // NOTE: child should be stopped after `ptrace::traceme()`, on its first instruction
            let _ = waitpid(child, None)?;
            ptrace::setoptions(child, options)?;
            ptrace::cont(child, None)?;

            // list of tracee pids
            let mut pids = vec![child];
            let mut rss = 0;
            loop {
                if pids.is_empty() {
                    break;
                }

                for i in 0..pids.len() {
                    let status = waitpid(pids[i], Some(WaitPidFlag::WNOHANG))?;

                    if !matches!(status, WaitStatus::StillAlive) {
                        eprintln!("::: {} {:?}", &pids[i], &status);
                    }

                    match status {
                        WaitStatus::Exited(pid, code) => {
                            println!("::: tracee {} exited with {}", pid, code);

                            // stop tracking this pid since the process will exit
                            pids.retain(|p| *p != pid);

                            // break here since we're iterating `pids` and we just changed its length
                            break;
                        }
                        WaitStatus::PtraceEvent(pid, _, value) => {
                            // this event fires early during process exit, so it's at this time we
                            // read the Rss value of the process just before it's gone
                            if value == Event::PTRACE_EVENT_EXIT as i32 {
                                rss += get_rss(pid)?;
                            }

                            // since we've set PTRACE_O_TRACE* options, all children will automatically
                            // be sent a SIGSTOP and will be made a tracee for us, so add them to our
                            // list of tracked pids and start handling them
                            const NEW_CHILD_EVENTS: [i32; 3] = [
                                Event::PTRACE_EVENT_FORK as i32,
                                Event::PTRACE_EVENT_VFORK as i32,
                                Event::PTRACE_EVENT_CLONE as i32,
                            ];
                            if NEW_CHILD_EVENTS.contains(&value) {
                                let new_pid = ptrace::getevent(pid)?;
                                let new_pid = Pid::from_raw(new_pid as i32);
                                pids.push(new_pid);
                            }

                            ptrace::cont(pid, None)?;
                        }
                        WaitStatus::Stopped(pid, _) => {
                            ptrace::cont(pid, None)?;
                        }
                        WaitStatus::StillAlive => {
                            thread::sleep(Duration::from_micros(100));
                        }
                        _ => {
                            ptrace::cont(pids[i], None)?;
                        }
                    }
                }
            }

            println!("max_rss: {}", rss);

            Ok(())
        }
        Err(e) => panic!("failed to fork: {}", e),
    }
}
