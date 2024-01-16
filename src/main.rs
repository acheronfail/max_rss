//! Some great references on how to use Linux's ptrace API:
//! - https://eli.thegreenplace.net/2011/01/23/how-debuggers-work-part-1/
//! - https://eli.thegreenplace.net/2011/01/27/how-debuggers-work-part-2-breakpoints
//! - https://eli.thegreenplace.net/2011/02/07/how-debuggers-work-part-3-debugging-information
//!
//! And some other good resources for understanding how to read process information:
//! - https://www.kernel.org/doc/html/latest/filesystems/proc.html?highlight=Pss#id10
//! - https://github.com/htop-dev/htop

mod cli;

use std::collections::HashMap;
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
use serde_json::{json, Value};

fn get_rss(pid: Pid) -> Result<u64> {
    #[cfg(debug_assertions)]
    eprintln!("rrr {}", pid);

    let path = format!("/proc/{}/smaps_rollup", pid);
    let smaps_rollup = fs::read_to_string(path)?;

    // extract line starting with "Rss:"
    let line = smaps_rollup
        .lines()
        .find(|x| x.starts_with("Rss:"))
        .expect("failed to find Rss line");

    // extract value: "Rss:      <VALUE> kb"
    let kb_str = line
        .split_ascii_whitespace()
        .nth(1)
        .expect("failed to find rss value");

    let kb = kb_str.parse::<u64>().expect("failed to parse rss value");
    Ok(kb * 1024)
}

#[derive(Debug, Default, Clone)]
struct ProcInfo {
    /// Whether this process has exited.
    exited: bool,

    /// All known children of this process.
    children: Vec<Pid>,

    /// Measured RSS for this process. Captured at the last moment before process exit.
    rss: u64,
}

fn tree(pid: Pid, table: &HashMap<Pid, ProcInfo>) -> Value {
    let info = table.get(&pid).expect("untracked pid");
    let children = info
        .children
        .iter()
        .map(|child| tree(*child, table))
        .collect::<Vec<_>>();

    json!({
        "id": pid.as_raw(),
        "rss": info.rss,
        "children": (!children.is_empty()).then(|| children)
    })
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

            // our exit code
            let mut exit_code = 0;

            // list of all currently known processes
            let mut procs = HashMap::new();
            procs.insert(child, ProcInfo::default());

            loop {
                // if all our processes have exited, we're done tracing
                if procs.iter().all(|(_, t)| t.exited) {
                    break;
                }

                // loop through each of our traced processes, and see if any have been stopped yet
                let pids_to_check = procs
                    .iter()
                    .filter_map(|(p, t)| if t.exited { None } else { Some(*p) })
                    .collect::<Vec<_>>();

                for current in pids_to_check {
                    // make sure we pass WNOHANG here so this check is non-blocking
                    let status = waitpid(current, Some(WaitPidFlag::WNOHANG))?;

                    #[cfg(debug_assertions)]
                    if !matches!(status, WaitStatus::StillAlive) {
                        eprintln!("::: {} {:?}", current, &status);
                    }

                    match status {
                        WaitStatus::Exited(pid, code) => {
                            // stop tracking this pid since the process exited
                            procs.entry(pid).and_modify(|i| i.exited = true);

                            if args.return_result && pid == child {
                                exit_code = code;
                            }
                        }
                        WaitStatus::Signaled(pid, signal, _) => {
                            // stop tracking this pid since the process exited
                            procs.entry(pid).and_modify(|i| i.exited = true);

                            if args.return_result && pid == child {
                                exit_code = 128 + signal as i32;
                            }
                        }
                        WaitStatus::PtraceEvent(pid, _, value) => {
                            // this event fires early during process exit, so it's at this time we
                            // read the Rss value of the process just before it's gone
                            if value == Event::PTRACE_EVENT_EXIT as i32 {
                                match procs.get_mut(&pid) {
                                    Some(i) => i.rss = get_rss(pid)?,
                                    None => unreachable!("untracked pid"),
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
                                procs.insert(new_pid, ProcInfo::default());
                                procs.entry(pid).and_modify(|i| i.children.push(new_pid));
                            }

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
                            ptrace::cont(current, None)?;
                        }
                    }
                }

                // delay a little here so we're not doing an extremely aggressive busy-wait-loop
                thread::sleep(Duration::from_micros(200));
            }

            let (max_rss, total_reads) = procs.iter().fold((0, 0), |acc, (pid, i)| {
                // this process created other processes, so count its rss towards our sum
                // this should be accurate enough, since linux uses copy-on-write for new
                // processes, even if a process forks 100 times, it won't use any more memory
                // (unless of course, a particular thread starts allocating more, etc)
                if *pid == child || !i.children.is_empty() {
                    (acc.0 + i.rss, acc.1 + 1)
                } else {
                    acc
                }
            });

            // write output file
            fs::write(
                args.output,
                format!(
                    "{}",
                    json!({
                        "max_rss": max_rss,
                        "total_pids": procs.len(),
                        "total_reads": total_reads,
                        "graph": tree(child, &procs)
                    })
                ),
            )?;

            process::exit(exit_code);
        }
        Err(e) => panic!("failed to fork: {}", e),
    }
}
