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
        .border_color(HAIRLINE)
        .child(
            div()
                .w(px(200.))
                .flex_none()
                .flex()
                .flex_col()
                .gap(px(4.))
                .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
                .child(div().text_size(px(12.)).text_color(TEXT_2).child(detail)),
        )
        .child(div().flex_1().flex().flex_wrap().items_center().gap(px(12.)).child(content))
}

fn list() -> Div {
    div().w(px(320.)).p(px(6.)).flex().flex_col().gap(px(2.)).rounded(px(16.)).bg(SURFACE_SUNKEN).shadow(vec![ui::ring(HAIRLINE, 0.5)])
}

fn swatch(label: &'static str, material: fn(Div) -> Div) -> Div {
    material(div().size(px(120.)).p(px(12.)).text_size(px(12.)).text_color(TEXT_2).child(label))
}

impl Render for Storybook {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tabs = vec![
            Segment { icon: None, value: Tab::Sessions, label: "Sessions".into(), badge: None },
            Segment { icon: None, value: Tab::Explore, label: "Explore".into(), badge: None },
            Segment { icon: None, value: Tab::Changes, label: "Changes".into(), badge: Some("3".into()) },
        ];
        let modes = vec![Segment { icon: None, value: Mode::Unified, label: "Unified".into(), badge: None }, Segment { icon: None, value: Mode::Split, label: "Split".into(), badge: None }];
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
                    .child(ui::button("cancel", Variant::Ghost, None, "Cancel"))
                    .child(ui::button("delete", Variant::Danger, None, "Delete")),
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
                    .child(ui::status("needs-you", State::NeedsYou))
                    .child(ui::status("working", State::Working))
                    .child(ui::status("failed", State::Failed))
                    .child(ui::status("sent", State::Sent))
                    .child(ui::status("draft", State::Draft))
                    .child(ui::status("done", State::Done(84, 51)))
                    .child(ui::status("idle", State::Idle(84, 51)))
                    .child(ui::status("not-attached", State::NotAttached)),
            ))
            .child(story(
                "Tags, keys",
                "Hairline tags",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::tag("2 sub-agents"))
                    .child(ui::kbd("⌘ K"))
                    .child(ui::kbd("↑ ↓")),
            ))
            .child(story(
                "Project mark",
                "Filled dark when selected, dot for needs you / done / working",
                div()
                    .flex()
                    .gap(px(12.))
                    .child(ui::repo_mark("app", false, None))
                    .child(ui::repo_mark("android", true, None))
                    .child(ui::repo_mark("ios", false, Some(State::NeedsYou)))
                    .child(ui::repo_mark("web", false, Some(State::Done(0, 0))))
                    .child(ui::repo_mark("hk", false, Some(State::Working))),
            ))
            .child(story(
                "Project row",
                "Sidebar rail",
                list()
                    .child(ui::repo_row("repo-android", ui::chevron("chev-android", true), "app-android", true, true))
                    .child(ui::repo_row("repo-ios", ui::chevron("chev-ios", false), "app-ios", false, true).children(ui::indicator("spin-ios", Some(State::NeedsYou))))
                    .child(ui::repo_row("repo-scratch", div().w(px(14.)).flex_none(), "scratch", false, false).children(ui::indicator("spin-scratch", Some(State::Working)))),
            ))
            .child(story(
                "Session row",
                "Sessions tab of the column",
                list()
                    .child(ui::session_row("s1", true, ui::provider_label("codex", false), "2m", "Fix stale terminal reveal".into(), None, Some("fix/restore-handoff".into()), Some(State::NeedsYou)))
                    .child(ui::session_row("s2", false, ui::provider_label("claude", false), "now", "Split restore hook into two files".into(), None, Some("refactor/restore-hook".into()), Some(State::Working)))
                    .child(ui::session_row("s3", false, ui::provider_label("codex", false), "3h", "Upgrade to RN 0.81".into(), None, Some("chore/rn-081".into()), Some(State::Failed)))
                    .child(ui::session_row("s4", false, ui::provider_label("codex", true), "6m", "Migrate legacy hooks".into(), None, Some("chore/migrate-hooks".into()), Some(State::Idle(28, 11)))),
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
                            .child(ui::tree_row("t1", "hooks".into(), true, true, 0, false, false, None))
                            .child(ui::tree_row("t2", "use-restore-preview.ts".into(), false, false, 1, true, true, Some('M')))
                            .child(ui::tree_row("t3", "use-terminal-settled.ts".into(), false, false, 1, false, false, Some('A'))),
                    )
                    .child(
                        list()
                            .child(ui::change_row("c1", ui::checkbox(true), "src/hooks/use-restore-preview.ts", true, 28, 11, 2))
                            .child(ui::change_row("c2", ui::checkbox(false), "src/components/terminal-view.tsx", false, 11, 5, 0)),
                    ),
            ))
            .child(story(
                "Rail",
                "Project tiles, add tile, inbox badge, avatar",
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(ui::repo_tile("AN", 38., true, None))
                    .child(ui::repo_tile("iO", 38., false, Some(State::NeedsYou)))
                    .child(ui::repo_tile("HK", 38., false, Some(State::Working)))
                    .child(ui::add_tile("rail-add", 38.))
                    .child(ui::count_badge(2))
                    .child(ui::avatar("MN", 32.)),
            ))
            .child(story(
                "Worktrees",
                "Nested under the selected project",
                list()
                    .child(ui::worktree_row("w1", "fix-login".into(), true))
                    .child(ui::worktree_row("w2", "brave-otter".into(), false).child(ui::row_trail(ui::indicator("w2-spin", Some(State::NeedsYou)), vec![ui::icon_button_sized("w2-more", "more", 22., TEXT_3).rounded(px(6.)).into_any_element()], false)))
                    .child(ui::worktree_row("w3", "add-dark-mode".into(), false).children(ui::indicator("w3-spin", Some(State::Working)))),
            ))
            .child(story(
                "Palette and menus",
                "Command palette rows and popover menu rows",
                list()
                    .child(ui::trigger_field("trigger", "search", "Search sessions, files and actions…", "⌘K"))
                    .child(ui::palette_row("p1", true, ui::dot(7., WAITING), "Fix stale terminal reveal", "app-android · Codex · waiting", None))
                    .child(ui::palette_row("p2", false, icon("sparkle", 13., TEXT_2), "New session in app-android", "", Some("⌘ N")))
                    .child(ui::menu_row("m1", "worktree", "New worktree…", Some("⌘ ⇧ N")))
                    .child(ui::menu_row("m2", "settings", "Project settings", Some("⌘ ,")))
                    .child(ui::menu_divider())
                    .child(ui::danger_row("m3", "trash", "Delete worktree…")),
            ))
            .child(story(
                "Form controls",
                "Fields, swatches, checkboxes, chips and links",
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.))
                    .child(ui::field_box().w(px(200.)).child(icon("branch", 13., TEXT_3)).child("main"))
                    .children(PALETTE.iter().map(|&c| ui::swatch(c, 22., 7.)))
                    .child(ui::checkbox(true))
                    .child(ui::checkbox(false))
                    .child(ui::chip("chip-all", true).child("All files"))
                    .child(ui::chip("chip-touched", false).child("Touched by agents"))
                    .child(ui::link("link", "+ Add file")),
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
            .bg(WINDOW)
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(TEXT)
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
