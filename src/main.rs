use std::{env, ffi::CString, fs, thread, time::Duration};

use anyhow::Result;
use nix::{
    sys::{
        ptrace::{self, Event, Options},
        signal::{
            raise,
            Signal::{SIGSTOP, SIGTRAP},
        },
        wait::{waitpid, WaitPidFlag, WaitStatus},
    },
    unistd::{execvp, fork, ForkResult, Pid},
};
use serde_json::json;

fn get_rss(proc: &Proc) -> Result<u64> {
    #[cfg(debug_assertions)]
    eprintln!("rrr {}", proc.pid);

    let path = format!("/proc/{}/smaps_rollup", proc.pid);
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

#[derive(Clone)]
struct Proc {
    pid: Pid,
    should_read: bool,
}

impl Proc {
    pub fn new(pid: Pid, should_read: bool) -> Proc {
        Proc { pid, should_read }
    }

    pub fn set_should_read(&mut self, state: bool) {
        self.should_read = state;
    }
}

fn main() -> Result<()> {
    match unsafe { fork() } {
        Ok(ForkResult::Child) => {
            let argv: Vec<CString> = env::args()
                .skip(1)
                .map(|s| CString::new(s).unwrap())
                .collect();

            ptrace::traceme()?;
            raise(SIGSTOP)?;

            execvp(&argv[0], &argv).expect_err("failed to execvp");

            Ok(())
        }
        Ok(ForkResult::Parent { child }) => {
            #[cfg(debug_assertions)]
            {
                println!("::: pid of tracer: {:?}", nix::unistd::getpid());
                println!("::: pid of tracee: {:?}", child);
            }

            let options = Options::PTRACE_O_TRACEEXIT
                | Options::PTRACE_O_TRACEFORK
                | Options::PTRACE_O_TRACEVFORK
                | Options::PTRACE_O_TRACECLONE;

            // NOTE: child should be stopped after `ptrace::traceme()`, on its first instruction
            let _ = waitpid(child, None)?;
            ptrace::setoptions(child, options)?;
            ptrace::cont(child, None)?;

            // list of tracee pids
            let mut proc_count = 1;
            let mut read_count = 0;
            let mut procs = vec![Proc::new(child, true)];
            let mut rss = 0;
            loop {
                if procs.is_empty() {
                    break;
                }

                for i in 0..procs.len() {
                    let status = waitpid(procs[i].pid, Some(WaitPidFlag::WNOHANG))?;

                    #[cfg(debug_assertions)]
                    if !matches!(status, WaitStatus::StillAlive) {
                        eprintln!("::: {} {:?}", &procs[i].pid, &status);
                    }

                    match status {
                        WaitStatus::Exited(pid, _) | WaitStatus::Signaled(pid, _, _) => {
                            // stop tracking this pid since the process will exit
                            procs.retain(|p| p.pid != pid);

                            // break here since we're iterating `pids` and we just changed its length
                            break;
                        }
                        WaitStatus::PtraceEvent(pid, _, value) => {
                            // this event fires early during process exit, so it's at this time we
                            // read the Rss value of the process just before it's gone
                            if value == Event::PTRACE_EVENT_EXIT as i32 {
                                let proc =
                                    procs.iter().find(|p| p.pid == pid).expect("untracked pid");

                                if proc.should_read {
                                    rss += get_rss(&proc)?;
                                    read_count += 1;
                                }
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
                                procs.push(Proc::new(new_pid, false));
                                proc_count += 1;
                            }

                            procs
                                .iter_mut()
                                .find(|p| p.pid == pid)
                                .unwrap()
                                .set_should_read(true);

                            ptrace::cont(pid, None)?;
                        }
                        WaitStatus::Stopped(pid, signal) => {
                            ptrace::cont(
                                pid,
                                if signal == SIGTRAP {
                                    None
                                } else {
                                    Some(signal)
                                },
                            )?;
                        }
                        WaitStatus::StillAlive => {
                            thread::sleep(Duration::from_micros(100));
                        }
                        _ => {
                            ptrace::cont(procs[i].pid, None)?;
                        }
                    }
                }
            }

            fs::write(
                "max_rss.json",
                format!(
                    "{}",
                    json!({
                        "max_rss": rss,
                        "total_pids": proc_count,
                        "total_reads": read_count
                    })
                ),
            )?;

            Ok(())
        }
        Err(e) => panic!("failed to fork: {}", e),
    }
}
