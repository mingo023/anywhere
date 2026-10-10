use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar};
use crate::add_to_chat::Quote;
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::empty_pane::Menu;
use crate::modals::new_session::picker::Picker;
use crate::desktop::chrome::{Confirm, Layout, Overlay, RowMenu, Screen, Side};
use crate::git_ui::diff::At;
use crate::git_ui::graph::GraphState;
use crate::settings::Section;
use crate::status::Status;
use crate::updates::Update;
use git::Kind;
use git::github::{Check, Checks, Comment, Mergeable, Outcome, Pr, PrState, Review, Thread};
use gpui_kit::component::Root;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use workspace::Doc;

type Step = fn(&mut Desktop, &mut Window, &mut Context<Desktop>);

const STEPS: [(&str, Step); 98] = [
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
        (d.side, d.sidebar.panel_open) = (Side::Explorer, true);
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
    ("automations", |d, window, cx| d.open_automations(&crate::actions::OpenAutomations, window, cx)),
    ("automations-runs", |d, _, _| d.automations.tab = crate::automations::logic::Tab::Runs),
    ("automations-done-run", |d, _, _| d.automations.selected_run = Some("r5".into())),
    ("automations-editor", |d, window, cx| {
        let first = d.agents.automations.items.first().map(|a| a.id.clone());
        d.edit_automation(first.as_deref(), window, cx);
    }),
    ("automations-new", |d, window, cx| d.edit_automation(None, window, cx)),
    ("settings", |d, window, cx| d.open_settings(&crate::actions::OpenSettings, window, cx)),
    ("settings-appearance", |d, _, _| d.settings.section = Section::Appearance),
    ("settings-notifications", |d, _, _| d.settings.section = Section::Notifications),
    ("settings-keyboard", |d, _, _| d.settings.section = Section::Keyboard),
    ("settings-projects", |d, _, _| d.settings.section = Section::Projects),
    ("settings-agents", |d, _, _| d.settings.section = Section::Agents),
    ("settings-automations", |d, _, _| d.settings.section = Section::Automations),
    ("settings-phone", |d, _, _| d.settings.section = Section::Phone),
    ("settings-sidebar", |d, _, _| d.settings.section = Section::Sidebar),
    ("settings-terminal", |d, _, _| d.settings.section = Section::Terminal),
    ("settings-files", |d, _, _| d.settings.section = Section::Files),
    ("settings-browser", |d, _, _| d.settings.section = Section::Browser),
    ("settings-git", |d, _, _| d.settings.section = Section::Git),
    ("settings-diff", |d, _, _| d.settings.section = Section::Diff),
    ("settings-claude", |d, _, _| d.settings.section = Section::Claude),
    ("settings-codex", |d, _, _| d.settings.section = Section::Codex),
    ("model-menu", |d, _, _| d.settings.menu = Some(match (d.overlay, d.settings.section) {
        (Some(Overlay::AddRepo), _) => "project-model",
        (_, Section::Git) => "ai-model",
        (_, Section::Codex) => "codex-model",
        _ => "claude-model",
    })),
    ("service-down", |d, _, _| d.terminals.link.down(Instant::now())),
    ("service-starting", |d, _, _| d.settings.starting = Some(Instant::now())),
    ("settings-search", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("sound", window, cx))),
    ("settings-search-chat", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("chat", window, cx))),
    ("settings-search-shell", |d, window, cx| d.settings.search.update(cx, |s, cx| s.set_value("shell", window, cx))),
    ("settings-advanced", |d, _, _| {
        let id = d.settings.section.id();
        d.store.advanced.insert(id.into());
    }),
    ("settings-reset", |d, _, _| {
        (d.store.sounds.all, d.store.notifications.banners) = (false, false);
        (d.confirm, d.overlay) = (Some(crate::desktop::chrome::Confirm::ResetSection(d.settings.section)), Some(Overlay::Confirm));
    }),
    ("palette", |d, window, cx| d.open(Overlay::Palette, window, cx)),
    ("new-session", |d, window, cx| d.open(Overlay::NewSession, window, cx)),
    ("form-model", |d, _, cx| d.toggle_picker(Picker::Model, cx)),
    ("form-access", |d, _, cx| d.toggle_picker(Picker::Access, cx)),
    ("empty-model", |d, _, cx| d.toggle_empty_menu(Menu::Model, cx)),
    ("empty-access", |d, _, cx| d.toggle_empty_menu(Menu::Access, cx)),
    ("new-worktree", |d, window, cx| d.new_worktree(&crate::actions::NewWorktree, window, cx)),
    ("prompt", |d, window, cx| d.reset_new_form(Some("The RestoreView snapshot fails on CI about 1 in 5 runs. Find out why and fix it, then run the tests.".into()), false, window, cx)),
    ("add-repo", |d, window, cx| d.open(Overlay::AddRepo, window, cx)),
    ("project-settings", |d, window, cx| d.project_settings(&crate::actions::ProjectSettings, window, cx)),
    ("phone-access", |d, window, cx| d.open(Overlay::PhoneAccess, window, cx)),
    ("pair-phone", |d, window, cx| d.open(Overlay::PairPhone, window, cx)),
    ("owner", |d, _, _| {
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
        d.agents.apply(agents::Event::Connected { scopes: strings(&["observe", "drive", "approve", "spawn", "owner"]), caps: strings(&["pair.v1", "scopes.v1"]), version: String::new() });
    }),
    ("phones", |d, _, _| {
        let hour = 3_600_000;
        let phone = |id: &str, name: &str, platform: &str, ago: i64| daemon::Device { id: id.into(), name: name.into(), platform: platform.into(), last_seen_at: crate::util::now_ms() - ago, legacy: false };
        d.settings.host.devices = Some(vec![phone("p1", "Minh's iPhone 16 Pro", "iOS 26.1", 0), phone("p2", "iPad Air", "iPadOS 26.0", 72 * hour)]);
    }),
    ("no-phones", |d, _, _| d.settings.host.devices = Some(Vec::new())),
    ("custom-shell", |d, window, cx| {
        (d.store.terminal.shell, d.store.terminal.shell_path) = (store::prefs::terminal::Shell::Custom, "/opt/homebrew/bin/fish".into());
        if let Some(field) = d.settings.fields.get("shell-args").cloned() {
            field.update(cx, |f, cx| f.set_value("--login --private", window, cx));
        }
        d.edit_terminal_color(9, window, cx);
    }),
    ("custom-search", |d, window, cx| {
        d.store.browser.engine = store::prefs::browser::Engine::Custom;
        if let Some(field) = d.settings.fields.get("custom-search").cloned() {
            field.update(cx, |f, cx| f.set_value("search.brave.com/search?q=", window, cx));
        }
    }),
    ("rename-phone", |d, window, cx| d.start_phone_rename("p1".into(), "Minh's iPhone 16 Pro", window, cx)),
    ("tailnet", |d, _, _| {
        d.agents.host = Some(agents::Host { tailnet: true, ..Default::default() });
        d.settings.host.status = Some(daemon::Status { listen: vec!["127.0.0.1:4517".into(), "100.88.12.4:4517".into()], ..Default::default() });
    }),
    ("advanced", |d, _, _| {
        d.store.advanced.insert(d.settings.section.id().to_string());
    }),
    ("custom-keys", |d, _, _| {
        d.store.keys.insert("desktop::NextNeedsYou".into(), "cmd-shift-u".into());
    }),
    ("key-clash", |d, _, _| d.settings.keyboard.recording = Some(("desktop::NewWorktree".into(), Some("cmd-k".into())))),
    ("dark", |d, window, cx| d.set_appearance(WindowAppearance::Dark, window, cx)),
    ("syntax-github", |d, _, cx| syntax(d, store::SyntaxTheme::Github, cx)),
    ("syntax-one", |d, _, cx| syntax(d, store::SyntaxTheme::One, cx)),
    ("syntax-solarized", |d, _, cx| syntax(d, store::SyntaxTheme::Solarized, cx)),
    ("custom-diff", |d, _, _| {
        (d.store.appearance.added, d.store.appearance.removed) = (Some(theme::DIFF_SWATCHES[3]), Some(theme::DIFF_SWATCHES[5]));
        theme::set_diff_colors(d.store.appearance.added, d.store.appearance.removed);
    }),
    ("accent-blue", |d, _, cx| {
        d.store.appearance.accent = Some(theme::ACCENT_SWATCHES[0]);
        theme::set_accent(d.store.appearance.accent, cx);
    }),
    ("ui-font-geist", |d, _, cx| {
        d.store.appearance.ui_font = Some("Geist".into());
        theme::set_fonts(Some("Geist"), d.store.appearance.code_font.as_deref(), cx);
    }),
    ("code-font-menlo", |d, _, cx| {
        d.store.appearance.code_font = Some("Menlo".into());
        theme::set_fonts(d.store.appearance.ui_font.as_deref(), Some("Menlo"), cx);
        d.diff.relayout(true);
    }),
    ("code-size-14", |d, _, _| {
        d.store.appearance.code_size = Some(14);
        d.diff.relayout(true);
    }),
    ("code-font-menu", |d, _, _| d.settings.menu = Some("code-font")),
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
        let Some(tree) = d.project.as_ref().and_then(|p| d.listed_trees(p)).and_then(|w| w.into_iter().find(|w| !w.main)).map(|w| w.path) else { return };
        d.removals.start(&tree);
    }),
    ("delete-worktree", |d, _, cx| {
        let Some((project, w)) = d.project.clone().and_then(|p| d.listed_trees(&p)?.into_iter().find(|w| !w.main).map(|w| (p, w))) else { return };
        let removal = crate::removal::Removal { project, tree: w.path, branch: Some(w.branch), delete_branch: true, teardown: true };
        d.confirm = Some(Confirm::DeleteWorktree { removal, dirty: 2, lost: 3 });
        d.overlay = Some(Overlay::Confirm);
        cx.notify();
    }),
    ("error", |d, _, _| d.error = Some("Couldn't save placeholder.tsx: Permission denied (os error 13)".into())),
    ("update-ready", |d, _, _| d.updates.update = Update::Ready { version: "0.2.0".into() }),
    ("whats-new", |d, _, _| d.updates.update = Update::WhatsNew { version: "0.2.0".into() }),
    ("prs", |d, _, _| {
        let Some(trees) = d.project.as_ref().and_then(|p| d.listed_trees(p)) else { return };
        let trees: Vec<String> = trees.into_iter().filter(|w| !w.main).map(|w| w.path).collect();
        let now = Instant::now();
        let checks = [Checks { passed: 4, ..Checks::default() }, Checks { passed: 2, failed: 1, ..Checks::default() }, Checks { passed: 1, pending: 2, ..Checks::default() }, Checks { passed: 5, ..Checks::default() }];
        for (i, tree) in trees.into_iter().enumerate() {
            let number = 120 + i as u32;
            let state = if i == 3 { PrState::Merged } else { PrState::Open };
            let pr = Pr { number, title: format!("Capture PR {number}"), url: format!("https://github.com/acme/app/pull/{number}"), state, checks: checks[i % 4], ..Pr::default() };
            d.prs.start(&tree, now);
            d.prs.apply(tree, now, Ok(Some(pr)), now);
        }
    }),
    ("pr-loop", |d, _, _| {
        let Some(tree) = d.cwd() else { return };
        let at = crate::util::now_ms() as u64;
        let said = |author: &str, body: &str, mins: u64| Comment { author: author.into(), body: body.into(), at: at - mins * 60_000, hunk: String::new() };
        let thread = |id: &str, line: u32, resolved: bool, comments: Vec<Comment>| Thread { id: id.into(), path: "hooks/use-restore-preview.ts".into(), line: Some(line), resolved, comments, ..Thread::default() };
        let run = |name: &str, outcome: Outcome, secs: u64| Check { name: name.into(), outcome, url: "https://github.com/acme/app/actions/runs/1".into(), secs: Some(secs) };
        let pr = Pr {
            number: 128,
            title: "Fix stale terminal reveal".into(),
            url: "https://github.com/acme/app/pull/128".into(),
            checks: Checks { passed: 3, failed: 1, pending: 0 },
            runs: vec![run("lint", Outcome::Passed, 42), run("typecheck", Outcome::Passed, 61), run("test", Outcome::Failed, 134), run("build", Outcome::Passed, 98)],
            review: Review::ChangesRequested,
            reviews: vec![("hoang".into(), Review::ChangesRequested)],
            requested: vec!["linh".into()],
            mergeable: Mergeable::Clean,
            base: "main".into(),
            head: "fix/restore-handoff".into(),
            threads: vec![
                thread("t1", 4, false, vec![said("hoang", "This reads the preview before the session restores, so the first frame is stale.", 40)]),
                thread("t2", 12, false, vec![said("hoang", "Can we drop the timeout here?", 35), said("Minh Ngo", "Dropped it in the next push.", 10)]),
                thread("t3", 18, true, vec![said("linh", "Nit: name it restorePreview.", 90)]),
            ],
            ..Pr::default()
        };
        let now = Instant::now();
        d.prs.start(&tree, now);
        d.prs.apply(tree, now, Ok(Some(pr)), now);
    }),
    ("pr-comments", |d, _, _| d.pr.track.comments = true),
    ("pr-diff", |d, _, cx| {
        let first = d.shown_pr().and_then(|(_, pr)| pr.threads.first().map(|t| t.id.clone()));
        if let Some(id) = first {
            d.view_thread(&id, cx);
        }
    }),
    ("names", |d, _, _| {
        let tree = d.project.as_ref().and_then(|p| d.listed_trees(p)).into_iter().flatten().find(|w| !w.main).map(|w| w.path);
        if let Some(tree) = tree {
            d.agents.names.insert(tree, "Fix the login form".into());
        }
    }),
    ("project-menu", |d, _, _| d.row_menu = d.project.clone().map(RowMenu::Project)),
    ("import", |d, _, cx| {
        if let Some(p) = d.project.clone() {
            d.import_worktrees(&p, cx);
        }
    }),
    ("rename", |d, window, cx| {
        if let Some(tree) = d.agents.names.keys().next().cloned() {
            d.start_rename(tree, window, cx);
        }
    }),
    ("daemon-down", |d, _, _| d.terminals.disconnected(Instant::now() - crate::terminals::link::HINT_AFTER)),
    ("service-off", |d, _, _| d.terminals.link.service = Some(daemon::service::Service::RequiresApproval)),
    ("service-updating", |d, _, _| d.terminals.link.connected("0.2.0", "0.1.0", true, Instant::now())),
    ("service-stale", |d, _, _| d.terminals.link.connected("0.2.0", "0.1.0", true, Instant::now() - crate::terminals::link::STALE_AFTER)),
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
    d.automations.reset();
    (d.screen, d.side) = (Screen::Sessions, Side::Sessions);
    d.settings.reset(window, cx);
    d.settings.menu = None;
    (d.layout, d.widths, d.sidebar.panel_open, d.panels.menu) = (Layout::Sidebars, [None; 2], false, None);
    (d.worktree, d.terminal.focused) = (None, None);
    for p in d.preview.panes.values_mut() {
        p.file = None;
    }
    for v in d.diff.panes.values_mut() {
        (v.file, v.at) = (None, At::Working);
    }
    for c in d.commit.panes.values_mut() {
        c.sha = None;
    }
    d.workspaces.clear();
    d.creates.list.clear();
    d.graph = GraphState::default();
    d.pr.track.comments = false;
    d.agents.names.clear();
    d.sidebar.rename = None;
    d.updates.update = Update::Idle;
    d.terminals.link.up();
    d.terminals.link.service = None;
    d.set_appearance(WindowAppearance::Light, window, cx);
    syntax(d, store::SyntaxTheme::Graphite, cx);
    (d.store.appearance.added, d.store.appearance.removed) = (None, None);
    theme::set_diff_colors(None, None);
    (d.store.appearance.accent, d.store.appearance.ui_font, d.store.appearance.code_font, d.store.appearance.code_size) = (None, None, None, None);
    theme::set_fonts(None, None, cx);
    theme::set_accent(None, cx);
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

fn syntax(d: &mut Desktop, palette: store::SyntaxTheme, cx: &mut Context<Desktop>) {
    d.store.appearance.syntax = palette;
    theme::set_syntax(palette as usize, cx);
}
