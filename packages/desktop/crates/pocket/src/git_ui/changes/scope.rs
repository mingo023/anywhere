use crate::desktop::Desktop;
use crate::git_ui::compare::{Choice, switch_label};
use crate::modals::new_session::picker::{menu_input, pick_head};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::scope::Scope;
use theme::*;

impl Desktop {
    /// The switch between Uncommitted and a Compare, with a Compare's line totals.
    pub(super) fn scope_bar(&self, scope: &Scope, against: Option<&str>, cx: &mut Context<Self>) -> Div {
        let cwd = self.cwd().unwrap_or_default();
        let read = against.and_then(|a| self.compare.view.get(&cwd, a)).and_then(|s| s.files.as_ref());
        let label = switch_label(scope, against);
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
        let totals = read.filter(|files| !files.is_empty()).map(|files| {
            let (added, removed) = git::totals(files);
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

    /// Uncommitted, and the branches to compare against found by the search.
    fn scope_menu(&self, scope: &Scope, cx: &mut Context<Self>) -> Stateful<Div> {
        let choices = self.compare_choices(cx);
        let mut rows = Vec::new();
        if !choices.is_empty() {
            rows.push(pick_head("Branches").into_any_element());
        }
        for (n, choice) in choices.into_iter().enumerate() {
            let (label, meta) = match &choice {
                Choice::Base(b) => (b.clone(), Some("base".to_string())),
                Choice::Branch(b) => (b.clone(), None),
            };
            let chosen = scope.compare && choice.against() == scope.against;
            let row = ui::pick_row(("compare-ref", n), chosen, Some(icon("branch", 13., TEXT_3)), div().font_family(MONO).text_size(px(12.5)).child(label), meta)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_compare(choice.clone(), cx)));
            rows.push(row.into_any_element());
        }
        let uncommitted = ui::menu_pick("compare-uncommitted", icon("list-flat", 13., TEXT_3), "Uncommitted", !scope.compare).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.compare.menu = false;
            this.change_scope(|s| s.compare = false, cx);
        }));
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
    }
}
