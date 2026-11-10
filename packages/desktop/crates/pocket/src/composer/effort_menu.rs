use super::model_menu::{GAP, PAD, check, row};
use super::{Draft, launch};
use crate::desktop::Desktop;
use crate::settings::models::effort_label;
use gpui_kit::*;
use theme::*;

const LIST_WIDTH: f32 = 148.;

/// Whether the list of levels is open, and the level last picked from it. The level outlives the list: a chat's effort
/// takes a poll to come back, and the row would show the old one until then.
#[derive(Default)]
pub(crate) struct EffortMenu {
    open: bool,
    picked: Option<String>,
}

impl EffortMenu {
    pub(super) fn toggle(&mut self) {
        self.open = !self.open;
    }

    fn pick(&mut self, level: String) {
        self.picked = Some(level);
        self.open = false;
    }

    /// Whether there was a list to close.
    pub(super) fn fold(&mut self) -> bool {
        std::mem::take(&mut self.open)
    }

    /// Which of `levels` the row names: the one picked, else `on`.
    fn shown(&self, levels: &[String], on: Option<&String>) -> Option<usize> {
        let at = |l: &String| levels.iter().position(|x| x == l);
        self.picked.as_ref().and_then(at).or_else(|| on.and_then(at))
    }
}

/// The panel's effort row and, while it's open, the list beside it.
pub(crate) struct Effort {
    pub(super) row: Stateful<Div>,
    pub(super) list: Option<Div>,
}

/// A row headed `word`, the agent's name for effort, naming the level in use, `on` unless `menu` holds a newer one, that
/// opens `levels` beside the panel. Picking one hands it to `pick`.
pub(crate) fn effort_menu(id: &'static str, word: &'static str, (levels, on): (&[String], Option<&String>), menu: &EffortMenu, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, pick: launch::Pick, cx: &mut Context<Desktop>) -> Effort {
    let at = menu.shown(levels, on);
    let name = at.map_or_else(|| "Default".to_string(), |i| effort_label(&levels[i]));
    let trigger = row(id, menu.open)
        .gap(px(6.))
        .pr(px(8.))
        .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(word))
        .child(div().text_color(TEXT_3).child(name))
        .child(icon("chevron-right", 14., TEXT_3))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            draft(this).effort.toggle();
            cx.notify();
        }));
    let list = menu.open.then(|| {
        let rows = levels.iter().enumerate().map(|(i, level)| {
            let (pick, level) = (pick.clone(), level.clone());
            let label = effort_label(&level);
            let on = at == Some(i);
            row(("effort-level", i), on).pr(px(6.)).child(div().flex_1().font_weight(FontWeight::MEDIUM).child(label)).child(check(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                draft(this).effort.pick(level.clone());
                pick(this, level.clone(), cx);
                cx.notify();
            }))
        });
        ui::pop(div()).w(px(LIST_WIDTH)).flex_none().p(px(PAD)).flex().flex_col().gap(px(GAP)).children(rows.collect::<Vec<_>>())
    });
    Effort { row: trigger, list }
}

#[cfg(test)]
mod tests {
    use super::EffortMenu;

    fn levels() -> Vec<String> {
        ["low", "medium", "high"].map(String::from).to_vec()
    }

    #[test]
    fn picking_a_level_closes_the_list() {
        let mut menu = EffortMenu::default();
        menu.toggle();
        menu.pick("high".to_string());
        assert!(!menu.open);
    }

    #[test]
    fn folding_says_whether_the_list_was_open() {
        let mut menu = EffortMenu::default();
        assert!(!menu.fold());
        menu.toggle();
        assert!(menu.fold());
        assert!(!menu.open);
    }

    #[test]
    fn the_row_names_the_level_picked_until_it_is_one_the_model_lacks() {
        let (levels, medium) = (levels(), "medium".to_string());
        let mut menu = EffortMenu::default();
        assert_eq!(menu.shown(&levels, None), None);
        assert_eq!(menu.shown(&levels, Some(&medium)), Some(1));
        menu.pick("high".to_string());
        assert_eq!(menu.shown(&levels, Some(&medium)), Some(2), "the chat's effort hasn't come back yet");
        assert_eq!(menu.shown(&levels[..2], Some(&medium)), Some(1));
    }
}
