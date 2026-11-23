use super::{Menu, RepoForm};
use crate::util::initials;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::ProjectIcon;
use theme::*;

pub(crate) const EMOJI: [&str; 40] = [
    "🚀", "⚡", "🔥", "✨", "🌟", "💎", "🎯", "🎨", "🧪", "🔬", "🛠️", "⚙️", "🧩", "📦", "📱", "💻", "🖥️", "🌐", "🔒", "🔑", "📊", "📈", "🧠", "🤖", "👾", "🎮",
    "🎵", "📚", "📝", "💡", "🌱", "🌊", "🍀", "🍎", "🍕", "☕", "🐙", "🦊", "🐳", "🦄",
];

const MARK: f32 = 76.;
const MARK_RADIUS: f32 = 20.;
const CELL: f32 = 30.;

impl RepoForm {
    /// The project's mark, which opens the icon menu: an emoji or picture as it is, else its initials on a plain tile.
    pub(super) fn mark_picker(&self, name: &str, cx: &mut Context<Self>) -> Div {
        let chosen = self.draft.icon.clone();
        let letters = initials(name);
        let mark = match &chosen {
            Some(ProjectIcon::Emoji(e)) => div().text_size(px(60.)).child(e.clone()),
            Some(ProjectIcon::Image(path)) => div().size_full().child(ui::Mark::Image { path: path.clone(), letters }.fill(MARK_RADIUS)),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(MARK_RADIUS))
                .bg(FILL_3)
                .shadow(vec![ui::ring(SEPARATOR, 0.5)])
                .text_size(px(28.))
                .font_weight(FontWeight::BOLD)
                .text_color(TEXT_2)
                .child(letters),
        };
        let badge = div()
            .absolute()
            .right(px(-5.))
            .bottom(px(-5.))
            .size(px(24.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(12.))
            .bg(POPOVER)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x00000040), 2., 6.)])
            .child(icon("pencil", 11., TEXT));
        let open = self.menu == Some(Menu::Mark);
        let button = ui::popover_trigger(
            div()
                .id("repo-icon")
                .relative()
                .size(px(MARK))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .active(|s| s.opacity(0.85))
                .child(mark)
                .child(badge),
            open,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = Some(Menu::Mark);
                cx.notify();
            }),
        );
        let menu = open.then(|| self.icon_menu(chosen, cx));
        div().relative().child(button).children(menu)
    }

    fn pick_image(&self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions { files: true, directories: false, multiple: true, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = rx.await else { return };
            this.update(cx, |f, cx| {
                if let Some(p) = paths.into_iter().next() {
                    f.draft.icon = Some(ProjectIcon::Image(p));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Emoji to pick from, then a picture from disk, then back to initials. Choosing the chosen emoji again clears it.
    fn icon_menu(&self, chosen: Option<ProjectIcon>, cx: &mut Context<Self>) -> Div {
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.menu = None;
            cx.notify();
        });
        let cells = EMOJI.iter().enumerate().map(|(i, &e)| {
            let on = chosen == Some(ProjectIcon::Emoji(e.into()));
            div()
                .id(("repo-emoji-cell", i))
                .size(px(CELL))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(6.))
                .text_size(px(18.))
                .cursor_pointer()
                .when(on, |d| d.bg(FILL_3))
                .hover(|d| d.bg(FILL_2))
                .child(e)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    let draft = &mut this.draft;
                    draft.icon = (draft.icon != Some(ProjectIcon::Emoji(e.into()))).then(|| ProjectIcon::Emoji(e.into()));
                    this.menu = None;
                    cx.notify();
                }))
        });
        let image = ui::menu_row("repo-icon-image", "folder", "Choose image\u{2026}", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.menu = None;
            this.pick_image(cx);
        }));
        let letters = chosen.is_some().then(|| {
            ui::menu_row("repo-icon-letters", "x", "Use initials", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.draft.icon = None;
                this.menu = None;
                cx.notify();
            }))
        });
        let width = 8. * CELL + 8.;
        let menu = ui::pop(div())
            .w(px(width))
            .p(px(4.))
            .flex()
            .flex_col()
            .child(div().flex().flex_wrap().children(cells))
            .child(ui::menu_divider())
            .child(image)
            .children(letters)
            .occlude()
            .on_mouse_down_out(close);
        // Centred under the mark, which `dropdown` can't do: it hangs from an edge.
        div()
            .absolute()
            .top(px(MARK + 10.))
            .left(px((MARK - width) / 2.))
            .child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(ui::menu_in("repo-emoji-menu-in", menu))).with_priority(1))
    }
}
