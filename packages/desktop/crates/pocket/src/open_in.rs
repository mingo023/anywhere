use crate::actions::CopyWorktreePath;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::explorer::toast::Notice;
use crate::modals::form::home;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use project::open_in::{self, Cmd, Entry, FINDER, Group, Installed, Opener};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use theme::*;

const MENU_W: f32 = 200.;
const SUB_W: f32 = 220.;
const GAP: f32 = 4.;

/// The submenus showing: a group's, and in it a list of editions.
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(crate) struct Flyouts {
    group: Option<&'static str>,
    sub: Option<&'static str>,
}

impl Flyouts {
    /// The group and the list of editions these name, among what's installed.
    fn shown<'a>(&self, groups: &'a [Group]) -> (Option<&'a Group>, Option<&'a [&'static Opener]>) {
        let group = groups.iter().find(|g| Some(g.id) == self.group);
        let editions = group.and_then(|g| {
            g.entries.iter().find_map(|e| match e {
                Entry::Editions { id, apps, .. } if Some(*id) == self.sub => Some(apps.as_slice()),
                _ => None,
            })
        });
        (group, editions)
    }
}

/// The apps a worktree can open in, and the button's menu.
#[derive(Default)]
pub(crate) struct OpenIn {
    apps: Installed,
    icons: HashMap<&'static str, PathBuf>,
    pub(crate) menu: Option<Flyouts>,
    over_chevron: bool,
}

impl OpenIn {
    /// Finder until the background probe says what else this Mac has.
    pub(crate) fn new(home: &Path, cx: &mut Context<Desktop>) -> Self {
        if cfg!(target_os = "macos") {
            let cache = home.join("app-icons");
            cx.spawn(async move |this, cx| {
                let apps = cx.background_spawn(async { installed_apps() }).await;
                let bundles = apps.clone();
                this.update(cx, |d, cx| {
                    d.open_in.apps = apps;
                    cx.notify();
                })
                .ok();
                let icons = cx.background_spawn(async move { icon_pngs(&bundles, &cache) }).await;
                this.update(cx, |d, cx| {
                    d.open_in.icons = icons;
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        Self { apps: open_in::installed(&[], |_| false), ..Self::default() }
    }

    /// True when it changed which submenus show.
    fn hover(&mut self, to: Flyouts) -> bool {
        match &mut self.menu {
            Some(shown) if *shown != to => {
                *shown = to;
                true
            }
            _ => false,
        }
    }

    /// True when the press closed the menu. A press on the chevron leaves that to its click, which would otherwise reopen it.
    fn press_outside(&mut self) -> bool {
        !self.over_chevron && self.menu.take().is_some()
    }
}

/// `cmd`'s output, when it ran and succeeded.
fn run(cmd: &Cmd) -> Option<String> {
    let out = daemon::command(cmd.program).args(&cmd.args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn installed_apps() -> Installed {
    open_in::installed(&open_in::search_roots(Path::new(&home())), |p| p.exists())
}

/// A PNG of each app's icon, for those that have one.
fn icon_pngs(apps: &Installed, cache: &Path) -> HashMap<&'static str, PathBuf> {
    let _ = std::fs::create_dir_all(cache);
    apps.iter().filter_map(|(id, bundle)| Some((*id, icon_png(id, bundle, cache)?))).collect()
}

fn icon_png(id: &str, bundle: &Path, cache: &Path) -> Option<PathBuf> {
    let png = cache.join(format!("{id}.png"));
    if png.exists() {
        return Some(png);
    }
    let icns = open_in::icns_path(bundle, &run(&open_in::icon_name(bundle))?)?;
    run(&open_in::png_icon(&icns, &png)).map(|_| png)
}

fn flyout(id: &'static str, rows: Vec<Stateful<Div>>) -> Stateful<Div> {
    ui::pop(div().id(id)).w(px(SUB_W)).p(px(6.)).flex().flex_col().children(rows)
}

impl Desktop {
    fn selected_opener(&self) -> &'static Opener {
        open_in::selected(&self.store.general.open_in, &self.open_in.apps)
    }

    fn open_folder_in(&self, app: &'static Opener, cx: &mut Context<Self>) {
        let (Some(folder), Some(bundle)) = (self.local_explore_root(), self.open_in.apps.get(app.id)) else { return };
        let launch = open_in::launch(app, bundle, &folder);
        cx.spawn(async move |this, cx| {
            if cx.background_spawn(async move { run(&launch).is_some() }).await {
                return;
            }
            this.update(cx, |d, cx| {
                d.error = Some(format!("Couldn't open this folder in {}", app.label));
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn pick_opener(&mut self, app: &'static Opener, cx: &mut Context<Self>) {
        self.store.general.open_in = app.id.to_string();
        self.open_in.menu = None;
        self.open_folder_in(app, cx);
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn copy_worktree_path(&mut self, _: &CopyWorktreePath, _: &mut Window, cx: &mut Context<Self>) {
        let Some(folder) = self.local_explore_root().filter(|_| self.screen == Screen::Sessions) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(folder.clone()));
        self.open_in.menu = None;
        self.show_notice(Notice::Copied(folder), cx);
    }

    fn toggle_open_in(&mut self, cx: &mut Context<Self>) {
        let opening = self.open_in.menu.is_none();
        self.close_menus();
        self.open_in.menu = opening.then(Flyouts::default);
        cx.notify();
    }

    fn hover_open_in(to: Flyouts, cx: &mut Context<Self>) -> impl Fn(&bool, &mut Window, &mut App) + 'static {
        cx.listener(move |this, hovered: &bool, _, cx| {
            if *hovered && this.open_in.hover(to) {
                cx.notify();
            }
        })
    }

    fn opener_icon(&self, app: &Opener, size: f32) -> AnyElement {
        match self.open_in.icons.get(app.id) {
            Some(png) => img(png.clone()).size(px(size)).flex_none().into_any_element(),
            None => icon(app.generic_icon, size, TEXT_2).into_any_element(),
        }
    }

    /// The top bar's button: the icon opens the selected tree's folder in the last app picked, the chevron opens the menu. `None` for a tree on a host or off macOS.
    pub(crate) fn open_in_button(&self, cx: &mut Context<Self>) -> Option<Div> {
        if !cfg!(target_os = "macos") {
            return None;
        }
        self.local_explore_root()?;
        let flyouts = self.open_in.menu;
        let open = div()
            .id("open-in-launch")
            .size(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .child(self.opener_icon(self.selected_opener(), 16.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_folder_in(this.selected_opener(), cx)));
        let chevron = div()
            .id("open-in-chevron")
            .w(px(18.))
            .h(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .when(flyouts.is_some(), |d| d.bg(FILL_2))
            .child(icon("chevron-down", 12., TEXT_3))
            .on_hover(cx.listener(|this, hovered: &bool, _, _| this.open_in.over_chevron = *hovered))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.toggle_open_in(cx)));
        Some(
            div()
                .relative()
                .flex_none()
                .flex()
                .items_center()
                // Else the bar's drag_area takes the press and moves the window.
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .child(open)
                .child(chevron)
                .when_some(flyouts, |d, flyouts| d.child(self.open_in_menu(flyouts, cx))),
        )
    }

    fn open_in_menu(&self, flyouts: Flyouts, cx: &mut Context<Self>) -> Div {
        let groups = open_in::menu(&self.open_in.apps);
        let (group, editions) = flyouts.shown(&groups);
        let copy = ui::menu_row("open-in-copy", "copy", "Copy path", Some("⌘⇧C"))
            .on_hover(Self::hover_open_in(Flyouts::default(), cx))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.copy_worktree_path(&CopyWorktreePath, window, cx)));
        let main = ui::pop(div().id("open-in-menu"))
            .w(px(MENU_W))
            .p(px(6.))
            .flex()
            .flex_col()
            .child(self.opener_row(&FINDER, Flyouts::default(), cx))
            .children(groups.iter().map(|g| Self::group_row(g, flyouts, cx)))
            .child(ui::menu_divider())
            .child(copy);
        let panel = group.map(|g| flyout("open-in-group", g.entries.iter().map(|e| self.entry_row(e, g.id, flyouts, cx)).collect()));
        let nested = editions.map(|apps| flyout("open-in-editions", apps.iter().map(|app| self.opener_row(*app, flyouts, cx)).collect()));
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            if this.open_in.press_outside() {
                cx.notify();
            }
        });
        let body = div().id("open-in-box").flex().flex_row_reverse().items_start().gap(px(GAP)).occlude().on_mouse_down_out(close).child(main).children(panel).children(nested);
        ui::dropdown_right(32., ui::menu_in("open-in-menu-in", body))
    }

    fn opener_row(&self, app: &'static Opener, to: Flyouts, cx: &mut Context<Self>) -> Stateful<Div> {
        let chosen = app.id == self.selected_opener().id;
        ui::menu_pick(format!("open-in-{}", app.id), self.opener_icon(app, 14.), app.label, chosen).on_hover(Self::hover_open_in(to, cx)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_opener(app, cx)))
    }

    fn submenu_row(id: String, lead: impl IntoElement, label: &str, open: bool, to: Flyouts, cx: &mut Context<Self>) -> Stateful<Div> {
        ui::menu_pick(id, lead, label, false).when(open, |d| d.bg(FILL_2)).child(icon("chevron-right", 12., TEXT_3)).on_hover(Self::hover_open_in(to, cx))
    }

    fn group_row(group: &Group, flyouts: Flyouts, cx: &mut Context<Self>) -> Stateful<Div> {
        Self::submenu_row(format!("open-in-{}", group.id), icon(group.icon, 14., TEXT_2), group.label, flyouts.group == Some(group.id), Flyouts { group: Some(group.id), sub: None }, cx)
    }

    fn entry_row(&self, entry: &Entry, group: &'static str, flyouts: Flyouts, cx: &mut Context<Self>) -> Stateful<Div> {
        let to = |sub| Flyouts { group: Some(group), sub };
        match entry {
            Entry::App(app) => self.opener_row(*app, to(None), cx),
            Entry::Editions { id, label, apps } => Self::submenu_row(format!("open-in-{id}"), self.opener_icon(apps[0], 14.), label, flyouts.sub == Some(*id), to(Some(*id)), cx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, FINDER, Flyouts, Group, OpenIn};

    fn shown() -> OpenIn {
        OpenIn { menu: Some(Flyouts::default()), ..OpenIn::default() }
    }

    #[test]
    fn flyouts_show_only_a_group_and_editions_that_are_installed() {
        let groups = [Group { id: "ide", label: "IDE", icon: "code", entries: vec![Entry::Editions { id: "vscode-editions", label: "VS Code", apps: vec![&FINDER] }] }];
        let (group, editions) = Flyouts { group: Some("ide"), sub: Some("vscode-editions") }.shown(&groups);
        assert_eq!((group.map(|g| g.id), editions.map(|apps| apps.len())), (Some("ide"), Some(1)));
        assert_eq!(Flyouts { group: Some("terminal"), sub: None }.shown(&groups), (None, None));
        assert_eq!(Flyouts { group: Some("ide"), sub: Some("jetbrains") }.shown(&groups).1, None);
    }

    #[test]
    fn hovering_a_row_reports_only_a_change_of_submenu() {
        let mut state = shown();
        let ide = Flyouts { group: Some("ide"), sub: None };
        assert!(state.hover(ide));
        assert!(!state.hover(ide));
        assert_eq!(state.menu, Some(ide));
    }

    #[test]
    fn a_closed_menu_ignores_hovers() {
        let mut state = OpenIn::default();
        assert!(!state.hover(Flyouts { group: Some("ide"), sub: None }));
        assert_eq!(state.menu, None);
    }

    #[test]
    fn a_press_outside_closes_the_menu_but_the_chevron_leaves_it_to_its_click() {
        let mut state = shown();
        state.over_chevron = true;
        assert!(!state.press_outside());
        assert!(state.menu.is_some());
        state.over_chevron = false;
        assert!(state.press_outside());
        assert_eq!(state.menu, None);
        assert!(!state.press_outside());
    }
}
