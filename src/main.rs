mod cli;

use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;
use std::{fs, process, thread};

use anyhow::Result;
use cli::Args;
use nix::sys::ptrace::{self, Event, Options};
use nix::sys::signal::raise;
use nix::sys::signal::Signal::{SIGSTOP, SIGTRAP};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::{execvp, fork, ForkResult, Pid};
use serde_json::json;

fn get_rss(proc: &Tracee) -> Result<u64> {
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
struct Tracee {
    /// PID of the process being traced
    pid: Pid,
    /// Whether we should count the RSS of this process towards our final sum
    should_read: bool,
}

impl Tracee {
    pub fn new(pid: Pid, should_read: bool) -> Tracee {
        Tracee { pid, should_read }
    }

    pub fn set_should_read(&mut self, state: bool) {
        self.should_read = state;
    }
}

fn main() -> Result<()> {
    let args = Args::parse()?;

    match unsafe { fork() } {
        // tracee
        Ok(ForkResult::Child) => {
            let argv = args
                .command
                .into_iter()
                .map(|s| CString::new(s.as_bytes()).unwrap())
                .collect::<Vec<CString>>();

            // become a tracee for the parent process
            ptrace::traceme()?;

            // immediately stop ourselves, so when the parent becomes our tracer
            // execution begins from here
            raise(SIGSTOP)?;

            // start the program to be traced
            execvp(&argv[0], &argv).expect_err("failed to execvp");

            Ok(())
        }

        // tracer
        Ok(ForkResult::Parent { child }) => {
            #[cfg(debug_assertions)]
            {
                println!("::: pid of tracer: {:?}", nix::unistd::getpid());
                println!("::: pid of tracee: {:?}", child);
            }

            // the child began by SIGSTOP'ing itself so we can attach to it now
            let _ = waitpid(child, None)?;
            // set our tracer options so we can intercept events of interest
            ptrace::setoptions(
                child,
                Options::PTRACE_O_TRACEEXIT
                    | Options::PTRACE_O_TRACEFORK
                    | Options::PTRACE_O_TRACEVFORK
                    | Options::PTRACE_O_TRACECLONE,
            )?;
            // now resume the child
            ptrace::cont(child, None)?;

            // total observed processes
            let mut proc_count = 1;
            // total processes we're including in the RSS sum
            let mut read_count = 0;
            // list of all currently known processes
            let mut procs = vec![Tracee::new(child, true)];
            // resident set size sum
            let mut rss = 0;
            // our exit code
            let mut exit_code = 0;

            loop {
                // no more processes to trace means everyone has exited, so we're done tracing
                if procs.is_empty() {
                    break;
                }

                // loop through each of our traced processes, and see if any have been stopped yet
                for i in 0..procs.len() {
                    // make sure we pass WNOHANG here so this check is non-blocking
                    let status = waitpid(procs[i].pid, Some(WaitPidFlag::WNOHANG))?;

                    #[cfg(debug_assertions)]
                    if !matches!(status, WaitStatus::StillAlive) {
                        eprintln!("::: {} {:?}", &procs[i].pid, &status);
                    }

                    match status {
                        WaitStatus::Exited(pid, code) => {
                            // stop tracking this pid since the process exited
                            procs.retain(|p| p.pid != pid);

                            if args.return_result && pid == child {
                                exit_code = code;
                            }

                            // break here since we're iterating `pids` and we just changed its length
                            break;
                        }
                        WaitStatus::Signaled(pid, signal, _) => {
                            // stop tracking this pid since the process exited
                            procs.retain(|p| p.pid != pid);

                            if args.return_result && pid == child {
                                exit_code = 128 + signal as i32;
                            }

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
                                procs.push(Tracee::new(new_pid, false));
                                proc_count += 1;
                            }

                            // this process created other processes, so count its rss towards our sum
                            // this should be accurate enough, since linux uses copy-on-write for new
                            // processes, even if a process forks 100 times, it won't use any more memory
                            // (unless of course, a particular thread starts allocating more, etc)
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
                                // if the signal was SIGTRAP then it was likely sent because of us as
                                // the tracer, but if it was something else, just send the signal
                                // through to the process
                                if signal == SIGTRAP {
                                    None
                                } else {
                                    Some(signal)
                                },
                            )?;
                        }
                        WaitStatus::StillAlive => {
                            // this pid is still running (has not been stopped) so just continue
                            // checking other pids
                            continue;
                        }
                        _ => {
                            // any other event we don't currently handle
                            ptrace::cont(procs[i].pid, None)?;
                        }
                    }
                }

                // delay a little here so we're not doing an extremely aggressive busy-wait-loop
                thread::sleep(Duration::from_micros(200));
            }

            // write output file
            fs::write(
                args.output,
                format!(
                    "{}",
                    json!({
                        "max_rss": rss,
                        "total_pids": proc_count,
                        "total_reads": read_count
                    })
                ),
            )?;

            process::exit(exit_code);
        }
        Err(e) => panic!("failed to fork: {}", e),
    }
}
