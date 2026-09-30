pub(crate) mod column;
pub(crate) mod rail;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Overlay, RowMenu, Screen, drag_area, id, state};
use crate::status::{self, Card};
use crate::util::basename;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, dot, icon_button_sized};

fn row_mark(key: &str, setting_up: bool, cards: &[Card]) -> Option<AnyElement> {
    if setting_up {
        Some(ui::setting_up(id(format!("aside-setup:{key}"))).into_any_element())
    } else {
        let rolled = status::roll_up(cards.iter().map(|c| c.status)).map(|(s, _)| state(s, 0, 0));
        ui::indicator(id(format!("aside-spin:{key}")), rolled)
    }
}

#[derive(Clone)]
struct DragProject {
    path: String,
    ix: usize,
}

pub struct SidebarState {
    pub(crate) search: Entity<InputState>,
}

impl SidebarState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let subs = vec![cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify())];
        (Self { search }, subs)
    }
}

impl Desktop {
    /// The expanded sidebar: projects with their worktrees.
    pub(crate) fn aside(&self, cx: &mut Context<Self>) -> Div {
        let top = drag_area(div())
            .h(px(42.))
            .flex()
            .flex_none()
            .items_center()
            .justify_end()
            .child(icon_button_sized("aside-bell", "bell", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_inbox(window, cx))));
        let search = ui::trigger_field("aside-search", "search", "Search", "⌘K")
            .mx(px(4.))
            .mb(px(6.))
            .h(px(32.))
            .rounded(px(10.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let header = div()
            .pt(px(10.))
            .pb(px(4.))
            .px(px(10.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(TEXT_3))
            .child(div().flex_1().child("Projects"))
            .child(
                icon_button_sized("aside-add", "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx))),
            );
        let mut repos = Vec::new();
        for (i, p) in self.projects().into_iter().enumerate() {
            repos.push(self.project_block(i, &p, cx));
        }
        let body = div().id("aside-repos").flex_1().min_h_0().overflow_y_scroll().flex().flex_col().gap(px(1.)).children(repos);
        let foot = div()
            .pt(px(8.))
            .px(px(2.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(
                ui::button("aside-add-repo", ui::Variant::Glass, Some("plus"), "Add project")
                    .flex_1()
                    .h(px(36.))
                    .rounded(px(17.))
                    .justify_center()
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::AddRepo, window, cx))),
            )
            .child(
                ui::glass(icon_button_sized("aside-settings", "settings", 34., TEXT).rounded(px(17.)))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.project_settings(&crate::actions::ProjectSettings, window, cx))),
            );
        let aside = ui::side(div())
            .w(px(self.width(Column::Projects, 272.)))
            .flex_none()
            .h_full()
            .px(px(8.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .child(top)
            .child(search)
            .child(header)
            .child(body)
            .children(self.usage_card())
            .child(foot);
        self.resizable(aside, Column::Projects, cx)
    }

    fn tree_cards(&self, project: &str, tree: &str) -> Vec<Card> {
        self.cards(project).into_iter().filter(|c| self.tree_of(&c.cwd).as_deref() == Some(tree)).collect()
    }

    fn setting_up(&self, tree: &str) -> bool {
        self.terminals.setups.values().any(|t| t == tree)
    }

    fn open_row_menu(menu: RowMenu, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            this.close_menus();
            this.row_menu = Some(menu.clone());
            cx.notify();
        })
    }

    /// A project's row, then its worktrees' rows while it is open.
    fn project_block(&self, i: usize, p: &str, cx: &mut Context<Self>) -> AnyElement {
        let current = self.cwd().filter(|_| self.screen == Screen::Sessions && self.project.as_deref() == Some(p));
        let kept = self.store.projects.iter().any(|k| k == p);
        let main = self.tree_of(p).unwrap_or_else(|| p.to_string());
        let git = self.worktrees.get(p).is_some_and(|w| !w.is_empty());
        let branched = self.worktrees.get(p).is_some_and(|w| w.iter().any(|w| !w.main));
        let open = branched && !self.store.collapsed.contains(p);
        let cards = if open { self.tree_cards(p, &main) } else { self.cards(p) };
        let setting_up = !open && self.worktrees.get(p).into_iter().flatten().any(|w| self.setting_up(&w.path));
        let lead = if branched {
            let target = p.to_string();
            ui::chevron(("aside-chevron", i), open)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.store.toggle(&target);
                    this.store.save();
                    cx.notify();
                }))
                .into_any_element()
        } else {
            div().w(px(14.)).flex_none().into_any_element()
        };
        let menu = RowMenu::Project(p.to_string());
        let mut buttons = Vec::new();
        if git {
            let target = p.to_string();
            buttons.push(
                icon_button_sized(id(format!("aside-plus:{p}")), "plus", 22., TEXT_3)
                    .rounded(px(6.))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        cx.stop_propagation();
                        this.new_worktree_in(target.clone(), window, cx);
                    }))
                    .into_any_element(),
            );
        }
        buttons.push(self.row_menu_button(p, menu.clone(), cx));
        let trail = ui::row_trail(row_mark(p, setting_up, &cards), buttons, self.row_menu.as_ref() == Some(&menu));
        let target = p.to_string();
        let selected = current.as_ref().is_some_and(|c| !open || *c == main);
        let row = ui::repo_row(("aside-repo", i), lead, &self.repo_name(p), selected, kept)
            .child(trail)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), None, cx)))
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .when(kept, |row| row.on_drag(DragProject { path: p.to_string(), ix: i }, |_, _, _, cx| cx.new(|_| EmptyView)));
        let mut out = vec![row.into_any_element()];
        if open {
            let trees: Vec<String> = self.worktrees[p].iter().filter(|w| !w.main).map(|w| w.path.clone()).collect();
            for tree in &trees {
                let menu = RowMenu::Tree { project: p.to_string(), tree: tree.clone() };
                let mark = row_mark(tree, self.setting_up(tree), &self.tree_cards(p, tree));
                let trail = ui::row_trail(mark, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu));
                let (target, path) = (p.to_string(), tree.clone());
                out.push(
                    ui::worktree_row(id(format!("aside-tree:{tree}")), basename(tree), current.as_ref() == Some(tree))
                        .child(trail)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.select_tree(target.clone(), Some(path.clone()), cx)))
                        .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
                        .into_any_element(),
                );
            }
        }
        let to = p.to_string();
        div()
            .flex()
            .flex_col()
            .gap(px(1.))
            .when(open, |d| d.pb(px(6.)))
            .children(out)
            .when(kept, |d| {
                d.drag_over::<DragProject>(move |s, d, _, _| if d.ix == i { s } else { s.shadow(ui::drop_line(d.ix < i)) }).on_drop(cx.listener(move |this, d: &DragProject, _, cx| {
                    this.store.move_project(&d.path, &to);
                    this.store.save();
                    cx.notify();
                }))
            })
            .into_any_element()
    }

    fn row_menu_button(&self, key: &str, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.row_menu.as_ref() == Some(&menu);
        let toggle = menu.clone();
        let button = icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)).capture_any_mouse_down(cx.listener(
            move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                this.terminal.tab_menu = false;
                cx.notify();
            },
        ));
        div()
            .relative()
            .child(button)
            .when(open, |d| {
                d.child(ui::dropdown(
                    26.,
                    ui::pop(div().id("aside-menu")).w(px(210.)).p(px(6.)).rounded(px(14.)).flex().flex_col()
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = None;
                            cx.notify();
                        }))
                        .children(self.row_menu_items(&menu, cx)),
                ))
            })
            .into_any_element()
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match menu.clone() {
            RowMenu::Project(p) => {
                let kept = self.store.projects.contains(&p);
                let target = p.clone();
                let first = if kept {
                    ui::menu_row("aside-menu-settings", "settings", "Settings…", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_project(target.clone(), cx);
                        this.project_settings(&crate::actions::ProjectSettings, window, cx);
                    }))
                } else {
                    ui::menu_row("aside-menu-keep", "check", "Keep in Pocket", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.keep_project(&target, cx);
                    }))
                };
                vec![
                    first.into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", if kept { "Remove from Pocket" } else { "Remove" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Tree { project, tree } => vec![
                ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.ask_delete_worktree(project.clone(), tree.clone(), cx);
                    }))
                    .into_any_element(),
            ],
        }
    }

    /// Context left in each provider's newest open session.
    fn usage(&self) -> Vec<(&'static str, u64)> {
        let latest = |p: &str| {
            let a = self.agents.list.iter().filter(|a| a.provider == p && a.status != "closed").max_by_key(|a| a.updated_at)?;
            self.agents.context_left(&a.id)
        };
        ["claude", "codex"].into_iter().filter_map(|p| latest(p).map(|left| (p, left))).collect()
    }

    fn usage_card(&self) -> Option<Div> {
        let parts: Vec<Div> =
            self.usage().into_iter().map(|(p, left)| div().flex().items_center().gap(px(6.)).child(dot(7., provider_color(p))).child(format!("{left}%"))).collect();
        (!parts.is_empty()).then(|| {
            div()
                .mt(px(8.))
                .mx(px(2.))
                .py(px(8.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(12.))
                .rounded(px(12.))
                .bg(rgba(0xffffff8c))
                .shadow(vec![ui::ring(FILL_3, 0.5)])
                .text_size(px(12.))
                .text_color(rgba(TEXT_2))
                .children(parts)
                .child(div().ml_auto().text_color(rgba(TEXT_4)).child("context left"))
        })
    }
}
