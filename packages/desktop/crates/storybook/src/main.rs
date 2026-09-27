use gpui_kit::*;
use theme::*;
use ui::{Segment, State, Variant};

#[derive(Clone, Copy, PartialEq)]
enum Tab {
    Sessions,
    Explore,
    Changes,
}

#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Unified,
    Split,
}

struct Storybook {
    tab: Tab,
    mode: Mode,
}

fn story(title: &'static str, detail: &'static str, content: impl IntoElement) -> Div {
    div()
        .py(px(20.))
        .flex()
        .gap(px(24.))
        .border_t_1()
        .border_color(rgba(HAIRLINE))
        .child(
            div()
                .w(px(200.))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(div().text_size(px(12.)).text_color(rgba(TEXT_2)).child(detail)),
        )
        .child(div().flex_1().flex().flex_wrap().items_center().gap(px(12.)).child(content))
}

fn list() -> Div {
    div().w(px(320.)).p(px(6.)).flex().flex_col().gap(px(2.)).rounded(px(16.)).bg(rgba(SURFACE_SUNKEN)).shadow(vec![ui::ring(HAIRLINE, 0.5)])
}

fn swatch(label: &'static str, material: fn(Div) -> Div) -> Div {
    material(div().size(px(120.)).p(px(12.)).text_size(px(12.)).text_color(rgba(TEXT_2)).child(label))
}

impl Render for Storybook {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = vec![
            Segment { value: Tab::Sessions, label: "Sessions".into(), badge: None },
            Segment { value: Tab::Explore, label: "Explore".into(), badge: None },
            Segment { value: Tab::Changes, label: "Changes".into(), badge: Some("3".into()) },
        ];
        let modes = vec![Segment { value: Mode::Unified, label: "Unified".into(), badge: None }, Segment { value: Mode::Split, label: "Split".into(), badge: None }];
        let board = div()
            .flex()
            .flex_col()
            .child(story(
                "Button",
                "accent · secondary · ghost",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::button("comment", Variant::Accent, None, "Comment").child(ui::button_kbd("⌘↵")))
                    .child(ui::button("resolve", Variant::Secondary, Some("check"), "Resolve"))
                    .child(ui::button("cancel", Variant::Ghost, None, "Cancel")),
            ))
            .child(story(
                "Icon button & group",
                "Toolbar actions float as glass capsules",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::icon_button("find", "search"))
                    .child(ui::icon_button("compose", "compose"))
                    .child(ui::icon_group([ui::group_button("split-right", "split-right"), ui::group_button("split-down", "split-down")])),
            ))
            .child(story(
                "Segmented control",
                "Column switcher and diff mode",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::segmented(tabs, self.tab, false, false, |this: &mut Self, v, cx| {
                        this.tab = v;
                        cx.notify();
                    }, cx))
                    .child(ui::segmented(modes, self.mode, true, false, |this: &mut Self, v, cx| {
                        this.mode = v;
                        cx.notify();
                    }, cx)),
            ))
            .child(story(
                "Status",
                "Session states",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::status("waiting", State::Waiting))
                    .child(ui::status("running", State::Running))
                    .child(ui::status("failed", State::Failed))
                    .child(ui::status("done", State::Done(84, 51))),
            ))
            .child(story(
                "Badges, tags, keys",
                "Agent badges; hairline tags",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::agent_badge("codex", "Codex"))
                    .child(ui::agent_badge("claude", "Claude"))
                    .child(ui::tag("2 sub-agents"))
                    .child(ui::kbd("⌘ K"))
                    .child(ui::kbd("↑ ↓")),
            ))
            .child(story(
                "Repository mark",
                "Filled dark when selected, dot for waiting / running",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::repo_mark("app", false, false, false))
                    .child(ui::repo_mark("android", true, false, false))
                    .child(ui::repo_mark("ios", false, true, false))
                    .child(ui::repo_mark("hk", false, false, true)),
            ))
            .child(story(
                "Repository row",
                "Sidebar rail",
                list()
                    .child(ui::repo_row("repo-android", "app-android", true, None, "spin-android"))
                    .child(ui::repo_row("repo-ios", "app-ios", false, Some(State::Waiting), "spin-ios"))
                    .child(ui::repo_row("repo-web", "web", false, Some(State::Running), "spin-web")),
            ))
            .child(story(
                "Session row",
                "Sessions tab of the column",
                list()
                    .child(ui::session_row("s1", true, "Fix stale terminal reveal".into(), State::Waiting, "codex", "fix/restore-handoff".into(), "2m".into()))
                    .child(ui::session_row("s2", false, "Split restore hook into two files".into(), State::Running, "claude", "refactor/restore-hook".into(), "now".into()))
                    .child(ui::session_row("s3", false, "Upgrade to RN 0.81".into(), State::Failed, "codex", "chore/rn-081".into(), "3h".into()))
                    .child(ui::session_row("s4", false, "Migrate legacy hooks".into(), State::Done(28, 11), "codex", "chore/migrate-hooks".into(), "6m".into())),
            ))
            .child(story(
                "File rows",
                "Explore tree and Changes list",
                div()
                    .flex()
                    .gap(px(12.))
                    .items_start()
                    .child(
                        list()
                            .child(ui::tree_row("t1", "hooks".into(), true, true, 0))
                            .child(ui::tree_row("t2", "use-restore-preview.ts".into(), false, false, 1))
                            .child(ui::tree_row("t3", "use-terminal-settled.ts".into(), false, false, 1)),
                    )
                    .child(
                        list()
                            .child(ui::change_row("c1", ui::checkbox(true), "src/hooks/use-restore-preview.ts", true, 28, 11))
                            .child(ui::change_row("c2", ui::checkbox(false), "src/components/terminal-view.tsx", false, 11, 5)),
                    ),
            ))
            .child(story(
                "Materials",
                "page · side · pop",
                div().flex().gap(px(16.)).child(swatch("page", ui::page)).child(swatch("side", ui::side)).child(swatch("pop", ui::pop)),
            ));
        div()
            .id("storybook")
            .size_full()
            .overflow_y_scroll()
            .p(px(24.))
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(rgba(TEXT))
            .child(ui::page(div().px(px(40.)).pt(px(40.)).pb(px(20.))).child(div().pb(px(20.)).text_size(px(28.)).font_weight(FontWeight::BOLD).child("Components")).child(board))
    }
}

fn main() {
    gpui_kit::application().with_assets(Assets).run(|cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        let bounds = Bounds::centered(None, size(px(1200.), px(900.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions { title: Some("Storybook".into()), ..Default::default() }),
            ..Default::default()
        };
        cx.open_window(opts, |_, cx| cx.new(|_| Storybook { tab: Tab::Sessions, mode: Mode::Unified })).expect("open window");
        cx.activate(true);
    });
}
