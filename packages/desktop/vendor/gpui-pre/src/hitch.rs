//! A log of scroll hitches, for lag that shows only now and then on someone else's machine.
//!
//! What a frame does (its phases, the views it redrew, the list rows it laid out, the tasks run before it) is kept
//! in memory for [`KEPT`]. A frame that comes late or draws slowly while a wheel scroll is under way writes what's
//! kept to the log file, so a smooth scroll writes nothing. Only the main thread records; elsewhere every call is a
//! no-op.

use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt::Write as _;
use std::fs::{self, File, OpenOptions};
use std::io::Write as _;
use std::panic::Location;
use std::path::PathBuf;
use std::sync::mpsc::{self, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const KEPT: Duration = Duration::from_secs(2);
/// A wheel event this recent means a scroll is under way: momentum events keep coming while a fling glides.
const SCROLLING: Duration = Duration::from_millis(300);
/// One frame at 120Hz.
const SLOW_FRAME: Duration = Duration::from_micros(8_333);
/// Three frames at 120Hz between a wheel event and the frame that shows it.
const LATE_FRAME: Duration = Duration::from_millis(25);
/// A main-thread task this long is kept.
const LONG_TASK: Duration = Duration::from_millis(2);
/// The log moves aside to `<name>.old` past this size.
const MAX_BYTES: u64 = 5 << 20;

struct State {
    kept: VecDeque<(Instant, String)>,
    out: Sender<String>,
    last_wheel: Option<Instant>,
    /// The first wheel event no frame has shown yet.
    unshown_wheel: Option<Instant>,
    task: Option<(Instant, &'static Location<'static>)>,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Starts logging hitches to `path`, opening each run with `header`. Call it on the main thread.
pub fn enable(path: PathBuf, header: String) {
    let (out, lines) = mpsc::channel::<String>();
    std::thread::Builder::new()
        .name("hitch-log".into())
        .spawn(move || write_all(path, lines))
        .ok();
    out.send(format!("=== {} {header}\n", unix_now())).ok();
    STATE.with_borrow_mut(|state| {
        *state = Some(State { kept: VecDeque::new(), out, last_wheel: None, unshown_wheel: None, task: None })
    });
}

fn write_all(path: PathBuf, lines: mpsc::Receiver<String>) {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir).ok();
    }
    let open = || OpenOptions::new().create(true).append(true).open(&path).ok();
    let mut file: Option<File> = open();
    let mut size = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    for text in lines {
        if size > MAX_BYTES {
            fs::rename(&path, path.with_extension("old")).ok();
            file = open();
            size = 0;
        }
        if let Some(file) = file.as_mut()
            && file.write_all(text.as_bytes()).is_ok()
        {
            size += text.len() as u64;
        }
    }
}

fn unix_now() -> String {
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("unix={}.{:03}", now.as_secs(), now.subsec_millis())
}

/// Keeps a line about the current frame; `line` runs only while logging.
pub(crate) fn note(line: impl FnOnce() -> String) {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        let now = Instant::now();
        while state.kept.front().is_some_and(|(at, _)| now - *at > KEPT) {
            state.kept.pop_front();
        }
        state.kept.push_back((now, line()));
    });
}

pub(crate) fn wheel() {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        let now = Instant::now();
        state.last_wheel = Some(now);
        state.unshown_wheel.get_or_insert(now);
    });
}

pub(crate) fn task_started(location: &'static Location<'static>) {
    STATE.with_borrow_mut(|state| {
        if let Some(state) = state {
            state.task = Some((Instant::now(), location));
        }
    });
}

pub(crate) fn task_ended() {
    let Some((took, location)) = STATE.with_borrow_mut(|state| {
        let (start, location) = state.as_mut()?.task.take()?;
        Some((start.elapsed(), location))
    }) else {
        return;
    };
    if took >= LONG_TASK {
        note(|| format!("task {}:{} {}", location.file(), location.line(), ms(took)));
    }
}

/// Records a drawn frame, and writes what's kept if it came late or slow during a scroll.
pub(crate) fn frame_drawn(took: Duration, context: impl FnOnce() -> String) {
    let late = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        let waited = state.unshown_wheel.take().map(|at| at.elapsed());
        let scrolling = state.last_wheel.is_some_and(|at| at.elapsed() < SCROLLING);
        Some((waited, scrolling))
    });
    let Some((waited, scrolling)) = late else { return };
    note(|| {
        let waited = waited.map(|w| format!(" wheel-waited={}", ms(w))).unwrap_or_default();
        format!("frame {}{waited} {}", ms(took), context())
    });
    let reason = match waited {
        Some(waited) if waited > LATE_FRAME => format!("wheel event waited {} for a frame", ms(waited)),
        _ if scrolling && took > SLOW_FRAME => format!("frame took {} while scrolling", ms(took)),
        _ => return,
    };
    write_kept(&reason);
}

fn write_kept(reason: &str) {
    STATE.with_borrow_mut(|state| {
        let Some(state) = state else { return };
        let now = Instant::now();
        let mut text = format!("--- {} hitch: {reason}\n", unix_now());
        for (at, line) in state.kept.drain(..) {
            writeln!(text, "{:>9} {line}", format!("-{}", ms(now - at))).ok();
        }
        state.out.send(text).ok();
    });
}

pub(crate) fn ms(d: Duration) -> String {
    format!("{:.1}ms", d.as_secs_f64() * 1000.)
}
