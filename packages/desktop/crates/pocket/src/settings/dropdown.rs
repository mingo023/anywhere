use crate::desktop::Desktop;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::hash::Hash;
use std::ops::Range;
use std::rc::Rc;
use theme::*;

const ROW: f32 = 26.;
/// The rows a menu shows before it scrolls.
const ROWS: f32 = 12.;

/// A row of a menu: a value to pick, or the name of the group under it.
#[derive(Debug, PartialEq)]
pub(crate) enum Row {
    Heading(String),
    Pick { value: String, label: String },
}

impl Row {
    pub(crate) fn picks(options: Vec<(String, String)>) -> Vec<Self> {
        options.into_iter().map(|(value, label)| Self::Pick { value, label }).collect()
    }

    fn text(&self) -> &str {
        match self {
            Self::Heading(text) | Self::Pick { label: text, .. } => text,
        }
    }
}

/// The row the menu is as wide as: the one with the longest text.
fn widest(rows: &[Row]) -> Option<usize> {
    rows.iter().enumerate().max_by_key(|(_, row)| row.text().chars().count()).map(|(i, _)| i)
}

/// `trigger` opening a menu while `settings.menu` is `id`; `pick` gets the chosen value. An agent can list hundreds of
/// models, so `rows` are made only once the menu is open and again when `from` changes, and only those in view are built.
#[allow(clippy::too_many_arguments)]
pub(crate) fn dropdown(
    id: &'static str,
    trigger: Stateful<Div>,
    from: impl Hash,
    rows: impl FnOnce() -> Vec<Row>,
    chosen: &str,
    d: &Desktop,
    pick: impl Fn(&mut Desktop, String, &mut Context<Desktop>) + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let open = d.settings.menu == Some(id);
    let trigger = trigger.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        this.settings.menu = (!open).then_some(id);
        this.settings.menu_scroll = Default::default();
        cx.notify();
    }));
    if !open {
        return div().relative().child(trigger);
    }
    let rows = d.settings.menu_rows.get((id, from), rows);
    let (pick, chosen, widest, count, scroll) = (Rc::new(pick), chosen.to_string(), widest(&rows), rows.len(), d.settings.menu_scroll.clone());
    let list = uniform_list(
        SharedString::from(format!("{id}-options")),
        count,
        cx.processor(move |_, range: Range<usize>, _, cx| {
            range
                .map(|i| {
                    let line = div().id((id, i)).w_full().h(px(ROW)).px(px(8.)).flex().items_center();
                    match &rows[i] {
                        Row::Heading(name) => line.items_end().pb(px(4.)).text_size(px(10.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(div().whitespace_nowrap().child(name.clone())),
                        Row::Pick { value, label } => {
                            let (pick, value) = (pick.clone(), value.clone());
                            line.gap(px(6.))
                                .rounded(px(5.))
                                .cursor_pointer()
                                .hover(|d| d.bg(FILL_2))
                                .text_size(px(13.))
                                .text_color(TEXT)
                                .child(div().w(px(14.)).flex_none().when(value == chosen, |d| d.child(icon("check", 13., TEXT))))
                                .child(div().whitespace_nowrap().child(label.clone()))
                                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                                    this.settings.menu = None;
                                    pick(this, value.clone(), cx);
                                    this.save_soon(cx);
                                    cx.notify();
                                }))
                        }
                    }
                })
                .collect::<Vec<_>>()
        }),
    )
    .track_scroll(&scroll)
    .with_sizing_behavior(ListSizingBehavior::Infer)
    .with_width_from_item(widest)
    .min_w(px(172.))
    .max_h(px(ROW * ROWS));
    let menu = ui::menu_in(SharedString::from(format!("{id}-menu-in")), ui::pop(div()).relative().p(px(4.)).child(list).vertical_scrollbar(&scroll));
    div().relative().child(trigger).child(ui::dropdown_right(30., menu))
}

#[cfg(test)]
mod tests {
    use super::{Row, widest};

    #[test]
    fn a_menu_is_as_wide_as_its_longest_label() {
        let options = [("a", "Opus"), ("b", "Claude Sonnet 4.6 (1M context)"), ("c", "Haiku")].map(|(v, l)| (v.to_string(), l.to_string())).to_vec();
        assert_eq!(widest(&Row::picks(options)), Some(1));
        assert_eq!(widest(&[Row::Pick { value: "a".into(), label: "Opus".into() }, Row::Heading("CHATGPT OR CODEX SUBSCRIPTION".into())]), Some(1));
        assert_eq!(widest(&[]), None);
    }
}
