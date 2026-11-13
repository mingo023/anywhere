use super::card::{item_glyph, pill};
use super::logic::{Col, Item};
use super::recap::recap;
use crate::desktop::Desktop;
use crate::git_ui::pull_request::glyph_slot;
use crate::git_ui::pull_requests::{detail, glyph as pr_glyph, tone};
use git::github::{Check, Outcome};
use crate::util::{ago, first_line, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// The files the Changes box lists; the rest are a click away in the session.
const FILES_SHOWN: usize = 5;
const PANEL_WIDTH: f32 = 440.;
const PANEL_INSET: f32 = 12.;

fn tool_line(name: &str, verb: &'static str, what: String) -> Div {
    div().flex().items_center().gap(px(7.)).text_size(px(12.)).line_height(px(18.6)).text_color(TEXT_3).child(icon(name, 13., TEXT_4)).child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(verb)).child(what)
}

fn files(n: usize) -> String {
    format!("{n} file{}", if n == 1 { "" } else { "s" })
}

/// Failed checks first, then running, passed and skipped, as GitHub orders them.
fn by_urgency(runs: &[Check]) -> Vec<&Check> {
    let mut sorted: Vec<&Check> = runs.iter().collect();
    sorted.sort_by_key(|c| match c.outcome {
        Outcome::Failed => 0,
        Outcome::Pending => 1,
        Outcome::Passed => 2,
        Outcome::Skipped => 3,
    });
    sorted
}

/// A box with a heading over its rows.
fn titled_box(head: Div) -> Div {
    div().rounded(px(10.)).border(px(0.5)).border_color(SEPARATOR_STRONG).overflow_hidden().flex().flex_col().child(head.flex().items_center().gap(px(7.)).px(px(12.)).py(px(8.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).border_b(px(0.5)).border_color(SEPARATOR))
}

impl Desktop {
    /// The drawer's head: status, title, where it runs, and the way into it.
    fn activity_panel_head(&self, item: &Item, cx: &mut Context<Self>) -> Div {
        let chip = div().h(px(20.)).pl(px(6.)).pr(px(7.)).flex().flex_none().items_center().gap(px(5.)).rounded(px(10.)).bg(FILL_3).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(item_glyph(item)).child(item.label());
        let title = div().mt(px(8.)).text_size(px(14.5)).line_height(px(19.)).font_weight(FontWeight::SEMIBOLD).child(item.title.clone());
        let meta = div().mt(px(5.)).flex().items_center().gap(px(8.)).text_size(px(11.5)).text_color(TEXT_3).child(self.activity_badge(item)).child(item.project_name.clone()).child(self.activity_agent(&item.provider)).child(ago(item.at, now_ms()));
        let branch = (!item.branch.is_empty()).then(|| div().mt(px(5.)).flex().items_center().gap(px(5.)).text_size(px(11.)).font_family(MONO).text_color(TEXT_3).child(icon("branch", 11., TEXT_3)).child(div().min_w_0().truncate().child(item.branch.clone())));
        let what = div().flex_1().min_w_0().flex().flex_col().items_start().child(chip).child(title).child(meta).children(branch);
        let go = item.id.clone();
        let open = div()
            .id("activity-open-session")
            .h(px(24.))
            .px(px(9.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(4.))
            .rounded(px(7.))
            .bg(PRIMARY)
            .text_color(ON_PRIMARY)
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .cursor_pointer()
            .hover(|s| s.opacity(0.9))
            .child("Open session")
            .child(div().text_size(px(11.)).font_weight(FontWeight::MEDIUM).opacity(0.55).child("↵"))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_previewed(&go, window, cx)));
        let close = ui::icon_button_sized("activity-panel-close", "x", 26., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.activity.previewed = None;
            cx.notify();
        }));
        div().flex_none().flex().items_start().gap(px(10.)).pt(px(14.)).pr(px(12.)).pb(px(12.)).pl(px(16.)).border_b(px(0.5)).border_color(SEPARATOR).child(what).child(open).child(close)
    }

    /// The last turn: the prompt, what it read and edited, and the last thing it said.
    fn activity_log(&self, item: &Item) -> Div {
        let r = recap(self.agents.timelines.get(&item.id).map_or(&[][..], Vec::as_slice));
        let asked = (!r.asked.is_empty()).then(|| div().flex().justify_end().child(div().max_w(relative(0.85)).px(px(11.)).py(px(7.)).rounded(px(12.)).bg(FILL_3).line_height(px(20.15)).child(first_line(&r.asked))));
        let read = (r.read > 0).then(|| tool_line("file", "Read", files(r.read)));
        let diff = self.repos.get(&item.tree).map(|repo| repo.totals()).filter(|&(a, d)| a + d > 0);
        let edited = (r.edited > 0).then(|| tool_line("pencil", "Edited", files(r.edited)).children(diff.map(|(a, d)| ui::meta_diff(a, d, 12.))));
        let last = if item.col() == Col::Working && !item.note.is_empty() {
            Some(div().flex().items_center().gap(px(7.)).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(WORKING).child(icon("spinner", 13., WORKING)).child(item.note.clone()))
        } else {
            (!r.said.is_empty()).then(|| div().line_height(px(20.15)).text_color(TEXT_BODY).child(first_line(&r.said)))
        };
        div().flex().flex_col().gap(px(10.)).text_size(px(13.)).line_height(px(20.)).children(asked).children(read).children(edited).children(last)
    }

    /// The worktree's uncommitted files, a few of them.
    fn activity_files(&self, item: &Item) -> Option<Div> {
        let repo = self.repos.get(&item.tree).filter(|r| !r.files.is_empty())?;
        let (added, removed) = repo.totals();
        let total = pill().font_family(MONO).child(ui::meta_diff(added, removed, 11.));
        let rows = repo.files.iter().take(FILES_SHOWN).enumerate().map(|(i, f)| {
            let row = div().h(px(28.)).px(px(12.)).flex().items_center().gap(px(8.)).font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(div().flex_1().min_w_0().truncate().child(f.path.clone())).child(ui::meta_diff(f.added, f.removed, 11.5));
            if i > 0 { row.border_t(px(0.5)).border_color(SEPARATOR) } else { row }
        });
        Some(titled_box(div().child("Changes").child(div().flex_1()).child(total)).children(rows))
    }

    /// The PR with each of its checks, as GitHub lists them; a check opens its log.
    fn activity_pr(&self, item: &Item, cx: &mut Context<Self>) -> Option<Div> {
        let pr = self.prs.get(&item.tree)?;
        let (name, color) = pr_glyph(pr);
        let (id, url) = (item.id.clone(), pr.url.clone());
        let open = ui::icon_button_sized("activity-pr-open", "external", 22., TEXT_3).tooltip(|window, cx| Tooltip::new("Open on GitHub").build(window, cx)).my(px(-4.)).mr(px(-6.)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr_link(&id, url.clone(), window, cx)));
        let summary = div().ml_auto().flex_none().font_weight(FontWeight::NORMAL).text_color(tone(pr)).child(detail(pr));
        let head = div().child(icon(name, 13., color)).child(div().min_w_0().truncate().child(format!("Pull request #{}", pr.number))).child(summary).child(open);
        let rows = by_urgency(&pr.runs).into_iter().enumerate().map(|(i, check)| {
            let glyph = match check.outcome {
                Outcome::Passed => icon("check", 13., SUCCESS).into_any_element(),
                Outcome::Failed => icon("x", 13., FAILED).into_any_element(),
                Outcome::Pending => spinner(("activity-check", i), 12., WAITING).into_any_element(),
                Outcome::Skipped => icon("run-skipped", 13., TEXT_4).into_any_element(),
            };
            let row = div().id(("activity-check", i)).h(px(28.)).px(px(12.)).flex().items_center().gap(px(8.)).text_size(px(12.)).text_color(TEXT_2).border_t(px(0.5)).border_color(SEPARATOR).child(glyph_slot(glyph)).child(div().flex_1().min_w_0().truncate().child(check.name.clone()));
            if check.url.is_empty() {
                return row;
            }
            let (id, log) = (item.id.clone(), check.url.clone());
            row.cursor_pointer().hover(|s| s.bg(FILL_2)).child(div().flex_none().text_size(px(11.5)).text_color(TEXT_4).child("Details")).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr_link(&id, log.clone(), window, cx)))
        });
        Some(titled_box(head).children(rows))
    }

    pub(super) fn activity_panel(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let id = self.activity.previewed.clone()?;
        let item = self.activity.grouped.item(&id)?;
        let body = div()
            .id("activity-panel-body")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(16.))
            .py(px(14.))
            .flex()
            .flex_col()
            .gap(px(14.))
            .children(self.activity_ask_box(item, 12.5, cx).map(|b| b.px(px(12.)).py(px(10.)).gap(px(8.))))
            .child(self.activity_log(item))
            .children(self.activity_files(item))
            .children(self.activity_pr(item, cx));
        let reply = div()
            .h(px(40.))
            .pl(px(14.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(12.))
            .bg(SURFACE_SOLID)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(&self.activity.reply).appearance(false).p_0()))
            .child(div().pr(px(6.)).text_size(px(11.)).text_color(TEXT_4).child("↵"));
        let panel = div()
            .id("activity-panel")
            .bg(POPOVER)
            .rounded(px(R_POPOVER))
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x00000080), 12., 32.)])
            .absolute()
            .top(px(52.))
            .right(px(PANEL_INSET))
            .bottom(px(12.))
            .w(px(PANEL_WIDTH))
            .flex()
            .flex_col()
            .occlude()
            .child(self.activity_panel_head(item, cx))
            .child(body)
            .child(div().flex_none().px(px(12.)).pb(px(12.)).child(reply));
        // Slides in from past the window's right edge; switching cards keeps it mounted, so only opening plays this.
        let away = PANEL_WIDTH + 2. * PANEL_INSET;
        Some(panel.with_animation("activity-panel-in", Animation::new(std::time::Duration::from_millis(440)).with_easing(ease_out_quint()), move |d, t| d.right(px(PANEL_INSET - away * (1. - t)))))
    }
}

#[cfg(test)]
mod tests {
    use super::by_urgency;
    use git::github::{Check, Outcome};

    #[test]
    fn failed_checks_come_first_then_running_passed_and_skipped() {
        let check = |name: &str, outcome| Check { name: name.into(), outcome, url: String::new() };
        let runs = [check("lint", Outcome::Passed), check("docs", Outcome::Skipped), check("test", Outcome::Failed), check("build", Outcome::Pending), check("go", Outcome::Passed)];
        let names: Vec<&str> = by_urgency(&runs).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["test", "build", "lint", "go", "docs"]);
    }
}
