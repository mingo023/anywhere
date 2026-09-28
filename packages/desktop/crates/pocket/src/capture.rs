use crate::{Desktop, Overlay, Screen, Side, ToggleFocus, ToggleRail, ToggleSidebar};
use git::Kind;
use gpui_kit::component::Root;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::Duration;

type Step = fn(&mut Desktop, &mut Window, &mut Context<Desktop>);

const STEPS: [(&str, Step); 12] = [
    ("session", |d, window, cx| {
        if let Some(card) = d.project.clone().and_then(|p| d.cards(&p).into_iter().next()) {
            d.select_session(card.id, window, cx);
        }
    }),
    ("rail", |d, window, cx| d.toggle_rail(&ToggleRail, window, cx)),
    ("sidebar", |d, window, cx| d.toggle_sidebar(&ToggleSidebar, window, cx)),
    ("focus", |d, window, cx| d.toggle_focus(&ToggleFocus, window, cx)),
    ("explore", |d, _, cx| {
        d.side = Side::Explorer;
        d.refresh_git(cx);
    }),
    ("changes", |d, _, cx| d.open_changes(None, cx)),
    ("comment", |d, window, cx| {
        if let Some(i) = d.diff.iter().position(|l| l.kind == Kind::Add) {
            d.open_comment(i, window, cx);
        }
    }),
    ("inbox", |d, window, cx| d.open_inbox(window, cx)),
    ("palette", |d, window, cx| d.open(Overlay::Palette, window, cx)),
    ("new-session", |d, window, cx| d.open(Overlay::NewSession, window, cx)),
    ("project-menu", |d, window, cx| d.open(Overlay::ProjectMenu, window, cx)),
    ("add-repo", |d, window, cx| d.open(Overlay::AddRepo, window, cx)),
];

/// `pocket-desktop --capture <dir> <name>=<step>,<step> …` renders each screen in an off-screen,
/// unfocused window, writes `<dir>/impl-<name>.png` and quits.
pub struct Capture {
    dir: PathBuf,
    screens: Vec<(String, Vec<Step>)>,
}

fn fail(msg: &str) -> ! {
    eprintln!("pocket-desktop: {msg}");
    std::process::exit(2)
}

impl Capture {
    pub fn from_args() -> Option<Self> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        let (flag, rest) = args.split_first()?;
        if flag != "--capture" {
            return None;
        }
        let Some((dir, specs)) = rest.split_first() else { fail("usage: --capture <dir> <name>=<step>,<step> …") };
        let screens = specs
            .iter()
            .map(|spec| {
                let (name, steps) = spec.split_once('=').unwrap_or((spec, ""));
                let steps = steps.split(',').filter(|s| !s.is_empty()).map(|s| {
                    STEPS.iter().find(|(n, _)| *n == s).map(|(_, f)| *f).unwrap_or_else(|| {
                        let known: Vec<&str> = STEPS.iter().map(|(n, _)| *n).collect();
                        fail(&format!("unknown step '{s}' in {spec}; known: {}", known.join(", ")))
                    })
                });
                (name.to_string(), steps.collect())
            })
            .collect();
        Some(Self { dir: dir.into(), screens })
    }

    /// Never ordered on screen; parked far away too, so the real cursor can't trigger hover styles.
    pub fn hide(opts: &mut WindowOptions) {
        if let Some(WindowBounds::Windowed(bounds)) = &mut opts.window_bounds {
            bounds.origin = point(px(-10000.), px(-10000.));
        }
        opts.show = false;
        opts.focus = false;
    }

    pub fn run(self, window: WindowHandle<Root>, cx: &mut App) {
        let desktop = window.read(cx).ok().and_then(|r| r.view().clone().downcast::<Desktop>().ok()).expect("desktop root");
        let window = AnyWindowHandle::from(window);
        desktop.update(cx, |d, _| d.capturing = true);
        cx.spawn(async move |cx| {
            settle(cx, window, &desktop, 1500).await;
            for (name, steps) in &self.screens {
                for step in std::iter::once(reset as Step).chain(steps.iter().copied()) {
                    cx.update_window(window, |_, window, cx| desktop.update(cx, |d, cx| step(d, window, cx))).ok();
                    settle(cx, window, &desktop, 300).await;
                }
                settle(cx, window, &desktop, 300).await;
                let path = self.dir.join(format!("impl-{name}.png"));
                match cx.update_window(window, |_, window, cx| shoot(window, cx, &path)) {
                    Ok(Ok(())) => println!("{}", path.display()),
                    Ok(Err(e)) => eprintln!("{name}: {e}"),
                    Err(e) => eprintln!("{name}: {e}"),
                }
            }
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
}

fn reset(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
    d.cancel_comment(window, cx);
    d.close_overlay(window, cx);
    (d.screen, d.side, d.wide, d.rail_open, d.focus) = (Screen::Sessions, Side::Sessions, false, false, false);
    (d.session, d.focused, d.diff_file, d.file) = (None, None, None, None);
}

/// Waits for the latest git refresh to land, then lays out a frame: nothing else draws a hidden window.
async fn settle(cx: &mut AsyncApp, window: AnyWindowHandle, desktop: &Entity<Desktop>, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
    for _ in 0..50 {
        if desktop.read_with(cx, |d, _| d.git_done == d.git_run) {
            break;
        }
        cx.background_executor().timer(Duration::from_millis(100)).await;
    }
    cx.update_window(window, |_, window, cx| draw(window, cx)).ok();
}

fn draw(window: &mut Window, cx: &mut App) {
    window.refresh();
    window.draw(cx).clear(cx);
}

fn shoot(window: &mut Window, cx: &mut App, path: &Path) -> Result<(), String> {
    draw(window, cx);
    save(window, path)
}

#[cfg(feature = "capture")]
fn save(window: &Window, path: &Path) -> Result<(), String> {
    let image = window.render_to_image().map_err(|e| e.to_string())?;
    image.save(path).map_err(|e| e.to_string())
}

#[cfg(not(feature = "capture"))]
fn save(_: &Window, _: &Path) -> Result<(), String> {
    Err("rebuild with --features capture".into())
}
