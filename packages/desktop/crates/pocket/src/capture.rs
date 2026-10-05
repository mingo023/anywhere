use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar};
use crate::add_to_chat::Quote;
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Layout, Overlay, Screen, Side};
use crate::git_ui::graph::GraphState;
use crate::settings::{Section, SettingsState};
use crate::status::Status;
use git::Kind;
use git::github::{Checks, Pr, PrState};
use gpui_kit::component::Root;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use workspace::Doc;

type Step = fn(&mut Desktop, &mut Window, &mut Context<Desktop>);

const STEPS: [(&str, Step); 35] = [
    ("session", |d, window, cx| {
        if let Some(card) = d.project.clone().and_then(|p| d.cards(&p).into_iter().min_by_key(|c| c.status != Status::NeedsYou)) {
            d.focus_agent(&card.id, window, cx);
        }
    }),
    ("worktree", |d, _, _| d.worktree = d.cwd().and_then(|cwd| d.worktree_of(&cwd)).map(|w| w.path.clone())),
    ("rail", |d, window, cx| d.toggle_rail(&ToggleRail, window, cx)),
    ("focus", |d, window, cx| d.toggle_focus(&ToggleFocus, window, cx)),
    ("sidebar", |d, window, cx| d.toggle_sidebar(&ToggleSidebar, window, cx)),
    ("projects", |d, window, cx| d.toggle_project_picker(window, cx)),
    ("explore", |d, _, cx| {
        d.side = Side::Explorer;
        d.refresh_git(cx);
    }),
    ("changes", |d, _, cx| {
        d.open_changes(None, false, cx);
        d.refresh_graph(cx);
    }),
    ("graph-expand", |d, _, cx| d.toggle_commit(0, cx)),
    ("graph-file", |d, _, cx| {
        if let Some(doc) = d.graph.file_doc(0, 0) {
            d.open_doc(doc, false, cx);
        }
    }),
    ("graph-commit", |d, _, cx| {
        if let Some(sha) = d.graph.commits.first().map(|c| c.sha.clone()) {
            d.open_doc(Doc::Commit(sha), false, cx);
        }
    }),
    ("tab-menu", |d, _, _| d.panels.menu = Some(d.focused_pane())),
    ("browser", |d, window, cx| d.open_browser(None, window, cx)),
    ("file", |d, _, cx| {
        if let Some(path) = d.cwd().zip(d.repo().and_then(|r| r.files.first())).map(|(root, f)| format!("{root}/{}", f.path)) {
            d.open_file(path, false, cx);
        }
    }),
    ("pill", |d, _, _| d.chat.pill = Some((d.focused_pane(), point(px(420.), px(260.))))),
    ("ask-file", |d, _, _| {
        let pane = d.focused_pane();
        let Some((path, text)) = d.preview.pane(pane).and_then(|f| Some((f.file.clone()?, f.text()?.to_string()))) else { return };
        let end = text.match_indices('\n').nth(2).map_or(text.len(), |(i, _)| i);
        d.chat.file = Quote::code(&crate::util::basename(&path), &text, 0..end).map(|q| (point(px(420.), px(260.)), q));
    }),
    ("add-to-chat", |d, window, cx| {
        let pane = d.focused_pane();
        if let Some(i) = d.diff.view(pane).and_then(|v| v.lines.iter().position(|l| l.kind == Kind::Add)) {
            d.open_composer(pane, i, window, cx);
        }
    }),
    ("inbox", |d, window, cx| d.open_inbox(window, cx)),
    ("settings", |d, window, cx| d.open_settings(&crate::actions::OpenSettings, window, cx)),
    ("settings-appearance", |d, _, _| d.settings.section = Section::Appearance),
    ("settings-keybindings", |d, _, _| d.settings.section = Section::Keybindings),
    ("settings-worktrees", |d, _, _| d.settings.section = Section::Worktrees),
    ("palette", |d, window, cx| d.open(Overlay::Palette, window, cx)),
    ("new-session", |d, window, cx| d.open(Overlay::NewSession, window, cx)),
    ("prompt", |d, window, cx| d.reset_new_form(Some("The RestoreView snapshot fails on CI about 1 in 5 runs. Find out why and fix it, then run the tests.".into()), false, window, cx)),
    ("add-repo", |d, window, cx| d.open(Overlay::AddRepo, window, cx)),
    ("phone-access", |d, window, cx| d.open(Overlay::PhoneAccess, window, cx)),
    ("pair-phone", |d, window, cx| d.open(Overlay::PairPhone, window, cx)),
    ("dark", |d, window, cx| d.set_appearance(WindowAppearance::Dark, window, cx)),
    ("creating", |d, _, _| {
        let Some(p) = d.project.clone() else { return };
        let now = Instant::now();
        let spec = serde_json::json!({"project": p, "checkout": {"new": {"name": "fix-flaky-snapshot", "base": "main", "copy": true, "setup": true}}, "prompt": "Fix the flaky snapshot"});
        let mut c = Create::new("capture".into(), format!("{p}.worktrees/fix-flaky-snapshot"), spec, now - Duration::from_secs(9));
        for (step, note, ago) in [("verify", "", 8.6), ("fetch", "", 8.3), ("fetch", "Couldn't fetch, using local main", 0.), ("worktree", "", 6.1), ("copy", "", 5.6), ("setup", "", 5.5)] {
            c.reach(step, note, now - Duration::from_secs_f32(ago));
        }
        d.worktree = Some(c.path.clone());
        d.creates.list.push(c);
    }),
    ("fail", |d, _, _| {
        if let Some(c) = d.creates.list.first_mut() {
            c.fail("Setup exited 1".into(), "npm ERR! missing script: setup".into(), Instant::now());
        }
    }),
    ("deleting", |d, _, _| {
        let Some(tree) = d.project.as_ref().and_then(|p| d.worktrees.get(p)).and_then(|w| w.iter().find(|w| !w.main)).map(|w| w.path.clone()) else { return };
        d.removals.start(&tree);
    }),
    ("delete-worktree", |d, _, cx| {
        let Some((project, w)) = d.project.clone().and_then(|p| d.worktrees.get(&p)?.iter().find(|w| !w.main).cloned().map(|w| (p, w))) else { return };
        let removal = crate::removal::Removal { project, tree: w.path, branch: Some(w.branch), delete_branch: true, teardown: true };
        d.confirm = Some(Confirm::DeleteWorktree { removal, dirty: 2, lost: 3 });
        d.overlay = Some(Overlay::Confirm);
        cx.notify();
    }),
    ("error", |d, _, _| d.error = Some("Couldn't save placeholder.tsx: Permission denied (os error 13)".into())),
    ("prs", |d, _, _| {
        let Some(trees) = d.project.as_ref().and_then(|p| d.worktrees.get(p)) else { return };
        let trees: Vec<String> = trees.iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
        let now = Instant::now();
        let checks = [Checks { passed: 4, ..Checks::default() }, Checks { passed: 2, failed: 1, ..Checks::default() }, Checks { passed: 1, pending: 2, ..Checks::default() }, Checks { passed: 5, ..Checks::default() }];
        for (i, tree) in trees.into_iter().enumerate() {
            let number = 120 + i as u32;
            let state = if i == 3 { PrState::Merged } else { PrState::Open };
            let pr = Pr { number, title: format!("Capture PR {number}"), url: format!("https://github.com/acme/app/pull/{number}"), state, draft: false, checks: checks[i % 4] };
            d.prs.start(&tree, now);
            d.prs.apply(tree, now, Ok(Some(pr)), now);
        }
    }),
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
    d.cancel_chat(window, cx);
    d.close_overlay(window, cx);
    (d.screen, d.side, d.settings) = (Screen::Sessions, Side::Sessions, SettingsState::default());
    (d.layout, d.widths, d.sidebar.column_hidden, d.panels.menu) = (Layout::Sidebars, [None; 2], false, None);
    (d.worktree, d.terminal.focused) = (None, None);
    for p in d.preview.panes.values_mut() {
        p.file = None;
    }
    for v in d.diff.panes.values_mut() {
        (v.file, v.at) = (None, None);
    }
    for c in d.commit.panes.values_mut() {
        c.sha = None;
    }
    d.workspaces.clear();
    d.creates.list.clear();
    d.graph = GraphState::default();
    d.set_appearance(WindowAppearance::Light, window, cx);
}

/// Waits for the latest git refresh to land, then lays out a frame: nothing else draws a hidden window.
async fn settle(cx: &mut AsyncApp, window: AnyWindowHandle, desktop: &Entity<Desktop>, ms: u64) {
    cx.background_executor().timer(Duration::from_millis(ms)).await;
    for _ in 0..50 {
        if desktop.read_with(cx, |d, _| d.git_done == d.git_run && !d.graph.busy()) {
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
