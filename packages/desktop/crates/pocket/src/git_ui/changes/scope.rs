use crate::desktop::Desktop;
use crate::git_ui::compare::{Choice, switch_label};
use crate::modals::new_session::picker::{menu_input, pick_head};
use crate::util::WORKTREE_ICON;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use git::compare::Range;
use store::scope::Scope;
use theme::*;

impl Desktop {
    /// The switch between Uncommitted and a Compare, with a Compare's line totals.
    pub(super) fn scope_bar(&self, scope: &Scope, range: Option<&Range>, cx: &mut Context<Self>) -> Div {
        let cwd = self.cwd().unwrap_or_default();
        let read = range.and_then(|r| self.compare.view.get(&cwd, r)).and_then(|s| s.comparison.as_ref());
        let label = switch_label(scope, range.map(|r| r.against.as_str()), read.map(|c| c.counts));
        let open = self.compare.menu;
        // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
        let switch = div()
            .id("changes-scope")
            .h(px(24.))
            .min_w_0()
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_size(px(12.))
            .text_color(TEXT_2)
            .when(open, |d| d.bg(FILL_3))
            .hover(|st| st.bg(FILL_2))
            .child(div().min_w_0().truncate().child(label))
            .child(icon("chevron-down", 10., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_compare_menu(window, cx);
            }));
        let totals = read.filter(|c| !c.files.is_empty()).map(|c| {
            let (added, removed) = git::totals(&c.files);
            ui::meta_diff(added, removed, 11.)
        });
        div()
            .h(px(30.))
            .flex_none()
            .pl(px(8.))
            .pr(px(14.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().relative().flex_1().min_w_0().flex().child(switch).when(open, |d| d.child(ui::dropdown(28., ui::menu_in("compare-menu-in", self.scope_menu(scope, cx))))))
            .children(totals)
    }

    /// Uncommitted, the refs to compare against found by the search, and how to compare.
    fn scope_menu(&self, scope: &Scope, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut rows = Vec::new();
        let mut head = None;
        for (n, choice) in self.compare_choices(cx).into_iter().enumerate() {
            let (title, glyph, label, meta) = match &choice {
                Choice::Base(r) => ("Base", "branch", r.clone(), None),
                Choice::Worktree(t) => ("Worktrees", WORKTREE_ICON, t.name.clone(), (t.name != t.branch).then(|| t.branch.clone())),
                Choice::Branch(r) => ("Branches", "branch", r.clone(), None),
                Choice::Typed(r) => ("Tag or commit", "search", format!("Compare with {r}"), None),
            };
            if head != Some(title) {
                rows.push(pick_head(title).into_any_element());
                head = Some(title);
            }
            let chosen = scope.compare && choice.against() == scope.against;
            let row = ui::pick_row(("compare-ref", n), chosen, Some(icon(glyph, 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), meta)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_compare(choice.clone(), cx)));
            rows.push(row.into_any_element());
        }
        let uncommitted = ui::menu_pick("compare-uncommitted", icon("list-flat", 13., TEXT_3), "Uncommitted", !scope.compare).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.compare.menu = false;
            this.change_scope(|s| s.compare = false, cx);
        }));
        let direct = ui::menu_pick("compare-direct", ui::checkbox(scope.direct), "Direct: diff the two tips", false)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.change_scope(|s| s.direct = !s.direct, cx)));
        let working = ui::menu_pick("compare-working", ui::checkbox(scope.working), "Include uncommitted", false)
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.change_scope(|s| s.working = !s.working, cx)));
        ui::pop(div().id("compare-menu"))
            .w(px(320.))
            .flex()
            .flex_col()
            .overflow_hidden()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.compare.menu = false;
                cx.notify();
            }))
            .child(div().p(px(5.)).child(uncommitted))
            .child(menu_input("search", &self.compare.search))
            .child(div().id("compare-refs").max_h(px(280.)).overflow_y_scroll().p(px(5.)).flex().flex_col().children(rows))
            .child(div().p(px(5.)).border_t(px(0.5)).border_color(SEPARATOR).flex().flex_col().child(direct).child(working))
    }
}
