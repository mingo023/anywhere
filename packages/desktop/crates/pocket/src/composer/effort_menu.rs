use super::model_menu::{GAP, PAD, check, row};
use super::{Draft, launch};
use crate::desktop::Desktop;
use crate::models::effort_label;
use gpui_kit::*;
use theme::*;

const LIST_WIDTH: f32 = 148.;

/// The row whose list of levels is open, by the word that heads it, and the level last picked from one. The level
/// outlives the list: a chat's effort takes a poll to come back, and the row would show the old one until then.
#[derive(Default)]
pub(crate) struct EffortMenu {
    open: Option<SharedString>,
    picked: Option<(SharedString, String)>,
}

impl EffortMenu {
    pub(super) fn toggle(&mut self, word: &SharedString) {
        self.open = (self.open.as_ref() != Some(word)).then(|| word.clone());
    }

    fn pick(&mut self, word: SharedString, level: String) {
        self.picked = Some((word, level));
        self.open = None;
    }

    /// Whether there was a list to close.
    pub(super) fn fold(&mut self) -> bool {
        self.open.take().is_some()
    }

    /// Which of `levels` the row headed `word` names: the one picked from it, else `on`.
    fn shown(&self, word: &str, levels: &[(String, String)], on: Option<&String>) -> Option<usize> {
        let at = |l: &String| levels.iter().position(|(x, _)| x == l);
        self.picked.as_ref().filter(|(w, _)| w == word).and_then(|(_, l)| at(l)).or_else(|| on.and_then(at))
    }
}

/// One of the panel's rows under its models and, while it's open, the list beside it.
pub(crate) struct Effort {
    pub(super) row: Stateful<Div>,
    pub(super) list: Option<Div>,
}

/// A row headed `word`, the agent's name for effort, naming the level in use, `on` unless `menu` holds a newer one, that
/// opens `levels` beside the panel. Picking one hands it to `pick`.
pub(crate) fn effort_menu(id: &'static str, word: &'static str, (levels, on): (&[String], Option<&String>), menu: &EffortMenu, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, pick: launch::Pick, cx: &mut Context<Desktop>) -> Effort {
    let levels: Vec<(String, String)> = levels.iter().map(|l| (l.clone(), effort_label(l))).collect();
    choice_menu(id, word.into(), (&levels, on), menu, draft, pick, cx)
}

/// `effort_menu` for any of a model's settings: `levels` are each value it takes and what the list calls it.
pub(crate) fn choice_menu(id: impl Into<ElementId>, word: SharedString, (levels, on): (&[(String, String)], Option<&String>), menu: &EffortMenu, draft: impl Fn(&mut Desktop) -> &mut Draft + Copy + 'static, pick: launch::Pick, cx: &mut Context<Desktop>) -> Effort {
    let at = menu.shown(&word, levels, on);
    let name = at.map_or_else(|| "Default".to_string(), |i| levels[i].1.clone());
    let open = menu.open.as_ref() == Some(&word);
    let heads = word.clone();
    let trigger = row(id, open)
        .gap(px(6.))
        .pr(px(8.))
        .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(word.clone()))
        .child(div().text_color(TEXT_3).child(name))
        .child(icon("chevron-right", 14., TEXT_3))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            draft(this).effort.toggle(&heads);
            cx.notify();
        }));
    let list = open.then(|| {
        let rows = levels.iter().enumerate().map(|(i, (level, label))| {
            let (pick, level, word) = (pick.clone(), level.clone(), word.clone());
            let on = at == Some(i);
            row(("effort-level", i), on).pr(px(6.)).child(div().flex_1().font_weight(FontWeight::MEDIUM).child(label.clone())).child(check(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                draft(this).effort.pick(word.clone(), level.clone());
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

    fn levels() -> Vec<(String, String)> {
        ["low", "medium", "high"].map(|l| (l.to_string(), l.to_string())).to_vec()
    }

    #[test]
    fn picking_a_level_closes_the_list() {
        let mut menu = EffortMenu::default();
        menu.toggle(&"Effort".into());
        menu.pick("Effort".into(), "high".to_string());
        assert!(menu.open.is_none());
    }

    #[test]
    fn folding_says_whether_the_list_was_open() {
        let mut menu = EffortMenu::default();
        assert!(!menu.fold());
        menu.toggle(&"Effort".into());
        assert!(menu.fold());
        assert!(menu.open.is_none());
    }

    #[test]
    fn the_row_names_the_level_picked_until_it_is_one_the_model_lacks() {
        let (levels, medium) = (levels(), "medium".to_string());
        let mut menu = EffortMenu::default();
        assert_eq!(menu.shown("Effort", &levels, None), None);
        assert_eq!(menu.shown("Effort", &levels, Some(&medium)), Some(1));
        menu.pick("Effort".into(), "high".to_string());
        assert_eq!(menu.shown("Effort", &levels, Some(&medium)), Some(2), "the chat's effort hasn't come back yet");
        assert_eq!(menu.shown("Effort", &levels[..2], Some(&medium)), Some(1));
    }

    #[test]
    fn one_row_s_list_is_open_at_a_time_and_a_pick_is_its_own_row_s() {
        let (levels, medium) = (levels(), "medium".to_string());
        let (effort, reasoning): (gpui_kit::SharedString, gpui_kit::SharedString) = ("Effort".into(), "Reasoning".into());
        let mut menu = EffortMenu::default();
        menu.toggle(&effort);
        menu.toggle(&reasoning);
        assert_eq!(menu.open, Some(reasoning.clone()));
        menu.toggle(&reasoning);
        assert_eq!(menu.open, None);
        menu.pick(reasoning, "high".to_string());
        assert_eq!((menu.shown("Reasoning", &levels, Some(&medium)), menu.shown("Effort", &levels, Some(&medium))), (Some(2), Some(1)));
    }
}
