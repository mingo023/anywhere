//! Runs a shell block's steps one after another on this computer, streaming what they print.

use futures::SinkExt;
use futures::channel::mpsc::Sender;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

/// Printed before each step, so the one shell running them all says which step it's on.
const MARK: u8 = 0x1e;

#[derive(Debug, PartialEq)]
pub(crate) enum Event {
    /// Step `n` (from 0) started.
    Step(usize),
    Output(String),
    Done(Exit),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Exit {
    Passed,
    /// Step `n` exited with this code, and the steps after it didn't run.
    Failed {
        step: usize,
        code: Option<i32>,
    },
    Stopped,
    /// The shell couldn't start.
    Unstarted(String),
}

/// Stops the run it was handed out for, killing the step running then. Dropping it stops the run too.
pub(crate) struct Stop(Arc<AtomicBool>);

impl Stop {
    pub(crate) fn stop(&self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl Drop for Stop {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Where an agent's shell blocks run: its folder, when that's on this computer and the app may act there. Only Unix
/// shells run them, since a step's progress is read from what `sh` prints.
pub(crate) fn runs_in(cwd: &str, observe_only: bool) -> Option<PathBuf> {
    (cfg!(unix) && !observe_only && !cwd.is_empty() && hosts::split(cwd).0.is_none()).then(|| PathBuf::from(cwd))
}

/// Runs `steps` in `cwd` in one shell, so a `cd` or `export` carries to the next, stopping at the first that fails. Events
/// go to `tx` until the run is done.
pub(crate) fn start(steps: Vec<String>, cwd: PathBuf, tx: Sender<Event>) -> Stop {
    let stop = Arc::new(AtomicBool::new(false));
    let stopping = stop.clone();
    std::thread::spawn(move || {
        let exit = run(&steps, &cwd, &stopping, &tx);
        _ = send(&tx, Event::Done(exit));
    });
    Stop(stop)
}

fn run(steps: &[String], cwd: &Path, stop: &AtomicBool, tx: &Sender<Event>) -> Exit {
    let mut child = match shell(&script(steps)).current_dir(cwd).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn() {
        Ok(child) => child,
        Err(e) => return Exit::Unstarted(e.to_string()),
    };
    let step = Arc::new(AtomicUsize::new(0));
    let readers: Vec<_> = [
        child.stdout.take().map(|o| (Box::new(o) as Box<dyn Read + Send>, Some((step.clone(), steps.len())))),
        child.stderr.take().map(|e| (Box::new(e) as _, None)),
    ]
    .into_iter()
    .flatten()
    .map(|(out, step)| {
        let tx = tx.clone();
        std::thread::spawn(move || stream(out, &tx, step.as_ref().map(|(at, of)| (&**at, *of))))
    })
    .collect();
    let status = loop {
        if stop.load(Ordering::Relaxed) {
            kill(&mut child);
            settle(&readers);
            return Exit::Stopped;
        }
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
            Err(e) => return Exit::Unstarted(e.to_string()),
        }
    };
    settle(&readers);
    match status.success() {
        true => Exit::Passed,
        false => Exit::Failed { step: step.load(Ordering::Relaxed), code: status.code() },
    }
}

/// The steps as one script, each after its mark. What the steps print to stderr joins stdout, so it lands under its step.
fn script(steps: &[String]) -> String {
    let marked: String =
        steps.iter().enumerate().map(|(n, step)| format!("printf '\\036{n}\\n'\n{step}\n__step=$?; [ $__step -eq 0 ] || exit $__step\n")).collect();
    format!("exec 2>&1\n{marked}")
}

/// Waits a moment for the last of what the shell printed: a process a step left in the background can hold its output open
/// for good, and its later output still streams in after the run is done.
fn settle(readers: &[std::thread::JoinHandle<()>]) {
    for _ in 0..20 {
        if readers.iter().all(|r| r.is_finished()) {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

/// Waits while the card is behind, so a step printing faster than it's drawn waits on its pipe rather than filling memory.
fn send(tx: &Sender<Event>, event: Event) -> Result<(), futures::channel::mpsc::SendError> {
    futures::executor::block_on(tx.clone().send(event))
}

/// Streams what a shell prints to `tx`; on stdout, `step` is where the step marks go and how many steps there are.
fn stream(mut out: Box<dyn Read + Send>, tx: &Sender<Event>, step: Option<(&AtomicUsize, usize)>) {
    let mut buf = [0u8; 4096];
    let mut held = Vec::new();
    while let Ok(n @ 1..) = out.read(&mut buf) {
        held.extend_from_slice(&buf[..n]);
        for event in take(&mut held, step.map_or(0, |(_, of)| of)) {
            if let (Event::Step(n), Some((at, _))) = (&event, step) {
                at.store(*n, Ordering::Relaxed);
            }
            if send(tx, event).is_err() {
                return;
            }
        }
    }
    if !held.is_empty() {
        _ = send(tx, Event::Output(String::from_utf8_lossy(&held).into_owned()));
    }
}

/// The events in what a shell printed so far, leaving in `held` a character or step mark cut off by the end of a read. Marks
/// are read only when there are `steps`, and one that names no step is output a step printed.
fn take(held: &mut Vec<u8>, steps: usize) -> Vec<Event> {
    let mut events = Vec::new();
    loop {
        let mark = (steps > 0).then(|| held.iter().position(|&b| b == MARK)).flatten();
        let text = &held[..mark.unwrap_or(held.len())];
        let whole = match (std::str::from_utf8(text), mark) {
            (Err(e), None) if e.error_len().is_none() => e.valid_up_to(),
            _ => text.len(),
        };
        if whole > 0 {
            events.push(Event::Output(String::from_utf8_lossy(&held[..whole]).into_owned()));
        }
        let line = mark.and_then(|m| held[m..].iter().position(|&b| b == b'\n').map(|nl| (m, m + nl)));
        let Some((m, nl)) = line else {
            held.drain(..whole);
            return events;
        };
        let n = std::str::from_utf8(&held[m + 1..nl]).ok().and_then(|n| n.parse().ok()).filter(|&n| n < steps);
        events.push(match n {
            Some(n) => Event::Step(n),
            None => Event::Output(String::from_utf8_lossy(&held[m..=nl]).into_owned()),
        });
        held.drain(..=nl);
    }
}

#[cfg(unix)]
fn shell(script: &str) -> Command {
    use std::os::unix::process::CommandExt;
    let mut c = Command::new(daemon::login_shell());
    // The user's login shell sets the PATH their terminal has, and bash reads the script, as the agent wrote it for one; the
    // script goes by env so no shell's quoting touches it. Its own process group lets a stop end what it started too.
    c.arg("-lc").arg("exec bash -c \"$POCKET_SCRIPT\"").env("POCKET_SCRIPT", script).process_group(0);
    c
}

#[cfg(windows)]
fn shell(script: &str) -> Command {
    let mut c = Command::new("cmd");
    c.arg("/C").arg(script);
    c
}

/// Asks the step's processes to end, and kills any still there after two seconds.
#[cfg(unix)]
fn kill(child: &mut Child) {
    let group = format!("-{}", child.id());
    let signal = |sig: &str| Command::new("kill").args([sig, &group]).stderr(Stdio::null()).status().is_ok_and(|s| s.success());
    signal("-TERM");
    for _ in 0..40 {
        // Reaped first, as an exited shell left unreaped still counts in its group.
        _ = child.try_wait();
        if !signal("-0") {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    signal("-KILL");
    _ = child.wait();
}

#[cfg(windows)]
fn kill(child: &mut Child) {
    _ = Command::new("taskkill").args(["/T", "/F", "/PID", &child.id().to_string()]).status();
    _ = child.wait();
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use futures::StreamExt;
    use futures::channel::mpsc;

    fn events(steps: &[&str]) -> Vec<Event> {
        let (tx, rx) = mpsc::channel(64);
        let _stop = start(steps.iter().map(|s| s.to_string()).collect(), std::env::temp_dir(), tx);
        until_done(rx)
    }

    /// Whether a process running `command` is still there a moment after the run ended.
    fn still_running(command: &str) -> bool {
        std::thread::sleep(Duration::from_millis(200));
        Command::new("pgrep").args(["-f", command]).output().is_ok_and(|o| o.status.success())
    }

    /// Up to the run's end: a process left in the background, the login shell's own included, can keep the channel open.
    fn until_done(mut rx: mpsc::Receiver<Event>) -> Vec<Event> {
        futures::executor::block_on(async {
            let mut events = Vec::new();
            while let Some(e) = rx.next().await {
                let done = matches!(e, Event::Done(_));
                events.push(e);
                if done {
                    break;
                }
            }
            events
        })
    }

    fn output(events: &[Event]) -> String {
        events.iter().filter_map(|e| if let Event::Output(o) = e { Some(o.as_str()) } else { None }).collect()
    }

    #[test]
    fn each_step_runs_in_turn_and_its_output_streams_back() {
        let e = events(&["echo one", "echo two >&2"]);
        assert_eq!(e.first(), Some(&Event::Step(0)));
        assert!(e.contains(&Event::Step(1)));
        assert_eq!(output(&e), "one\ntwo\n");
        assert_eq!(e.last(), Some(&Event::Done(Exit::Passed)));
    }

    #[test]
    fn a_failing_step_ends_the_run_with_its_code() {
        let e = events(&["exit 3", "echo never"]);
        assert!(!e.contains(&Event::Step(1)));
        assert_eq!(e.last(), Some(&Event::Done(Exit::Failed { step: 0, code: Some(3) })));
    }

    #[test]
    fn stopping_kills_the_running_step() {
        let (tx, rx) = mpsc::channel(64);
        let stop = start(vec!["sleep 30".into()], std::env::temp_dir(), tx);
        std::thread::sleep(Duration::from_millis(200));
        stop.stop();
        let e = until_done(rx);
        assert_eq!(e.last(), Some(&Event::Done(Exit::Stopped)));
    }

    #[test]
    fn only_a_folder_on_this_computer_runs_blocks() {
        assert_eq!(runs_in("/repo", false), Some(PathBuf::from("/repo")));
        assert_eq!(runs_in("/repo", true), None);
        assert_eq!(runs_in("", false), None);
    }

    #[test]
    fn a_cd_carries_to_the_next_step() {
        let e = events(&["mkdir -p pocket-run-cd && cd pocket-run-cd", "basename \"$PWD\""]);
        assert!(output(&e).ends_with("pocket-run-cd\n"), "{}", output(&e));
    }

    #[test]
    fn a_later_step_failing_is_the_one_named() {
        let e = events(&["true", "exit 4"]);
        assert_eq!(e.last(), Some(&Event::Done(Exit::Failed { step: 1, code: Some(4) })));
    }

    #[test]
    fn stopping_kills_a_step_that_ignores_being_asked() {
        let (tx, rx) = mpsc::channel(64);
        let stop = start(vec!["trap '' TERM; sleep 31.1".into()], std::env::temp_dir(), tx);
        std::thread::sleep(Duration::from_millis(300));
        let asked = std::time::Instant::now();
        stop.stop();
        let e = until_done(rx);
        assert_eq!(e.last(), Some(&Event::Done(Exit::Stopped)));
        assert!(asked.elapsed() < Duration::from_secs(5));
        assert!(!still_running("sleep 31.1"), "sleep 31.1 outlived the stop");
    }

    #[test]
    fn a_step_left_in_the_background_doesnt_hold_the_run_open() {
        let (tx, mut rx) = mpsc::channel(64);
        let _stop = start(vec!["sleep 5 &".into(), "echo up".into()], std::env::temp_dir(), tx);
        let started = std::time::Instant::now();
        let done = futures::executor::block_on(async {
            while let Some(e) = rx.next().await {
                if let Event::Done(exit) = e {
                    return Some(exit);
                }
            }
            None
        });
        assert_eq!(done, Some(Exit::Passed));
        assert!(started.elapsed() < Duration::from_secs(3));
    }

    #[test]
    fn stopping_kills_what_ignores_being_asked_after_the_shell_is_gone() {
        let (tx, rx) = mpsc::channel(64);
        let stop = start(vec!["(trap '' TERM; exec sleep 31.2) & wait".into()], std::env::temp_dir(), tx);
        std::thread::sleep(Duration::from_millis(300));
        let asked = std::time::Instant::now();
        stop.stop();
        let e = until_done(rx);
        assert_eq!(e.last(), Some(&Event::Done(Exit::Stopped)));
        assert!(asked.elapsed() < Duration::from_secs(5));
        assert!(!still_running("sleep 31.2"), "sleep 31.2 outlived the stop");
    }

    #[test]
    fn a_step_printing_a_mark_past_the_last_step_is_only_output() {
        let e = events(&["printf '\\03699\\n'"]);
        assert!(!e.contains(&Event::Step(99)));
        assert!(output(&e).contains("\x1e99"));
        assert_eq!(e.last(), Some(&Event::Done(Exit::Passed)));
    }

    #[test]
    fn a_character_cut_by_a_read_waits_for_its_other_half() {
        let mut held = "é".as_bytes()[..1].to_vec();
        assert_eq!(take(&mut held, 0), []);
        held.extend_from_slice(&"é".as_bytes()[1..]);
        assert_eq!(take(&mut held, 0), [Event::Output("é".into())]);
    }

    #[test]
    fn a_step_mark_is_read_out_of_the_output_even_when_cut() {
        let mut held = b"a\n\x1e1".to_vec();
        assert_eq!(take(&mut held, 20), [Event::Output("a\n".into())]);
        held.extend_from_slice(b"2\nb");
        assert_eq!(take(&mut held, 20), [Event::Step(12), Event::Output("b".into())]);
    }

    #[test]
    fn a_separator_that_names_no_step_stays_in_the_output() {
        let mut held = b"\x1ehello\n\x1e7\n".to_vec();
        assert_eq!(take(&mut held, 3), [Event::Output("\x1ehello\n".into()), Event::Output("\x1e7\n".into())]);
    }
}
