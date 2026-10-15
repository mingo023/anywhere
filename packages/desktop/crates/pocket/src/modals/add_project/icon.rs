use crate::desktop::Desktop;
use crate::util::initials;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::ProjectIcon;
use theme::*;

pub(crate) const EMOJI_MENU: &str = "repo-emoji";

pub(crate) const EMOJI: [&str; 40] = [
    "🚀", "⚡", "🔥", "✨", "🌟", "💎", "🎯", "🎨", "🧪", "🔬", "🛠️", "⚙️", "🧩", "📦", "📱", "💻", "🖥️", "🌐", "🔒", "🔑", "📊", "📈", "🧠", "🤖", "👾", "🎮", "🎵", "📚", "📝", "💡", "🌱", "🌊", "🍀", "🍎", "🍕", "☕", "🐙", "🦊", "🐳", "🦄",
];

/// A choice, ringed in the project's colour while chosen.
fn choice(id: &'static str, on: bool, color: u32, tile: Div) -> Stateful<Div> {
    div().id(id).size(px(28.)).flex().items_center().justify_center().rounded(px(9.)).cursor_pointer().when(on, |d| d.shadow(vec![ui::ring(rgba(color), 1.5)])).child(tile)
}

impl Desktop {
    /// An emoji from a menu or a picked image; with neither, the project shows its initials. Choosing the chosen one again clears it.
    pub(super) fn icon_choices(&self, name: &str, cx: &mut Context<Self>) -> Div {
        let draft = &self.repo_form.draft;
        let (color, chosen) = (draft.color, draft.icon.clone());
        let letters = initials(name);
        let tile = |mark: ui::Mark| ui::colour_tile(&mark, color, 22., 7.);
        let emoji_tile = match &chosen {
            Some(ProjectIcon::Emoji(e)) => tile(ui::Mark::Emoji(e.clone())),
            _ => tile(ui::Mark::Emoji(EMOJI[0].into())).bg(FILL_2).opacity(0.5),
        };
        let open = self.settings.menu == Some(EMOJI_MENU);
        let by_emoji = choice("repo-icon-emoji", matches!(chosen, Some(ProjectIcon::Emoji(_))), color, emoji_tile).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.settings.menu = (!open).then_some(EMOJI_MENU);
            cx.notify();
        }));
        let image_tile = match &chosen {
            Some(ProjectIcon::Image(path)) => tile(ui::Mark::Image { path: path.clone(), letters }),
            _ => tile(ui::Mark::Letters(String::new())).bg(FILL_2).child(icon("plus", 12., TEXT_3)),
        };
        let image_chosen = matches!(chosen, Some(ProjectIcon::Image(_)));
        let by_image = choice("repo-icon-image", image_chosen, color, image_tile).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.settings.menu = None;
            if image_chosen {
                this.repo_form.draft.icon = None;
                cx.notify();
                return;
            }
            this.pick_path(false, window, cx, |d, paths, _, _| {
                if let Some(p) = paths.into_iter().next() {
                    d.repo_form.draft.icon = Some(ProjectIcon::Image(p));
                }
            })
        }));
        let menu = open.then(|| {
            let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.settings.menu = None;
                cx.notify();
            });
            let cells = EMOJI.iter().enumerate().map(|(i, &e)| {
                div()
                    .id(("repo-emoji-cell", i))
                    .size(px(30.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(6.))
                    .text_size(px(18.))
                    .cursor_pointer()
                    .hover(|d| d.bg(FILL_2))
                    .child(e)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let draft = &mut this.repo_form.draft;
                        draft.icon = (draft.icon != Some(ProjectIcon::Emoji(e.into()))).then(|| ProjectIcon::Emoji(e.into()));
                        this.settings.menu = None;
                        cx.notify();
                    }))
            });
            ui::dropdown_right(36., ui::menu_in("repo-emoji-menu-in", ui::pop(div()).w(px(8. * 30. + 8.)).p(px(4.)).flex().flex_wrap().children(cells).occlude().on_mouse_down_out(close)))
        });
        div().relative().h(px(38.)).flex().items_center().gap(px(4.)).child(by_emoji).child(by_image).children(menu)
    }
}
