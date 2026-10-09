use super::picker::access_row;
use crate::desktop::Desktop;
use crate::settings::models::{model_label, model_options};
use gpui_kit::*;
use std::rc::Rc;
use store::prefs::agents::ACCESSES;
use theme::*;

/// Sets a composer's model or access to the value picked.
pub(crate) type Pick = Rc<dyn Fn(&mut Desktop, String, &mut Context<Desktop>)>;

/// Each of `ACCESSES` as (label, hint); sized by it, so a new access won't build without its text.
const ACCESS_TEXT: [(&str, &str); ACCESSES.len()] = [
    ("Agent's settings", "Use the agent's own permission config."),
    ("Ask", "Ask before commands and file changes."),
    ("Edits", "Auto-approve edits, ask before other actions."),
    ("Auto", "A reviewer model approves or denies actions."),
    ("Full", "Skip every approval."),
];

fn access_text(access: &str) -> (&'static str, &'static str) {
    ACCESSES.iter().position(|a| *a == access).map_or(ACCESS_TEXT[0], |i| ACCESS_TEXT[i])
}

/// Full skips every approval, so it stands out.
fn access_color(access: &str) -> Token {
    if access == "full" { WAITING_TEXT } else { TEXT }
}

/// The model a launch starts with: the one picked, else `app`'s from Settings.
pub(crate) fn model_shown(picked: &str, app: &str) -> String {
    match (picked, app) {
        ("", "") => "Default model".into(),
        ("", app) => model_label(app),
        (picked, _) => model_label(picked),
    }
}

/// The access a launch starts with: the one picked, else `app`'s from Settings.
pub(crate) fn access_shown(picked: &str, app: &str) -> &'static str {
    access_text(if picked.is_empty() { app } else { picked }).0
}

/// The model menu's (value, label): Settings' first as "", then `provider`'s.
pub(crate) fn model_choices(provider: &str, codex: &[(String, String)], picked: &str, app: &str) -> Vec<(String, String)> {
    let none = if app.is_empty() { "Default".to_string() } else { format!("Default · {}", model_label(app)) };
    model_options(provider, codex, picked, &none)
}

/// The access menu's (value, label, hint): Settings' first as "", then each of `ACCESSES`.
pub(crate) fn access_choices(app: &str) -> Vec<(String, String, String)> {
    let default = (String::new(), format!("Default · {}", access_text(app).0), "Settings › Permission mode".to_string());
    std::iter::once(default).chain(ACCESSES.iter().map(|a| {
        let (label, hint) = access_text(a);
        (a.to_string(), label.to_string(), hint.to_string())
    })).collect()
}

/// A composer chip led by `glyph`; `toggle` opens or closes its menu.
pub(crate) fn chip(id: &'static str, open: bool, glyph: &str, label: impl Into<SharedString>, color: Token, toggle: impl Fn(&mut Desktop, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Stateful<Div> {
    ui::menu_chip(id, open)
        .child(icon(glyph, 13., TEXT_2))
        .child(div().font_weight(FontWeight::MEDIUM).text_color(color).child(label.into()))
        .child(icon("chevron-down", 12., TEXT_4))
        .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            toggle(this, cx);
        }))
}

pub(crate) fn access_chip(id: &'static str, open: bool, picked: &str, app: &str, toggle: impl Fn(&mut Desktop, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let access = if picked.is_empty() { app } else { picked };
    chip(id, open, "shield", access_shown(picked, app), access_color(access), toggle, cx)
}

pub(crate) fn model_rows(id: &'static str, choices: Vec<(String, String)>, picked: &str, pick: Pick, cx: &mut Context<Desktop>) -> Vec<AnyElement> {
    choices
        .into_iter()
        .enumerate()
        .map(|(i, (value, label))| {
            let pick = pick.clone();
            ui::pick_row((id, i), value == picked, None::<Div>, div().child(label), None)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| pick(this, value.clone(), cx)))
                .into_any_element()
        })
        .collect()
}

pub(crate) fn access_rows(id: &'static str, choices: Vec<(String, String, String)>, picked: &str, pick: Pick, cx: &mut Context<Desktop>) -> Vec<AnyElement> {
    choices
        .into_iter()
        .enumerate()
        .map(|(i, (value, label, hint))| {
            let pick = pick.clone();
            access_row((id, i), value == picked, None, &label, &hint, access_color(&value))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| pick(this, value.clone(), cx)))
                .into_any_element()
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{access_choices, access_shown, model_choices, model_shown};

    #[test]
    fn a_chip_names_the_pick_else_what_settings_start_with() {
        assert_eq!((model_shown("", ""), model_shown("", "opus"), model_shown("haiku", "opus")), ("Default model".to_string(), "Opus".to_string(), "Haiku".to_string()));
        assert_eq!((access_shown("", "settings"), access_shown("", "auto"), access_shown("full", "auto")), ("Agent's settings", "Auto", "Full"));
    }

    #[test]
    fn each_menu_leads_with_settings_own_choice() {
        assert_eq!(model_choices("claude", &[], "", "opus")[0], (String::new(), "Default · Opus".to_string()));
        assert_eq!(model_choices("claude", &[], "", "")[0].1, "Default");
        let access: Vec<_> = access_choices("ask").into_iter().map(|(v, l, _)| (v, l)).collect();
        assert_eq!(access[0], (String::new(), "Default · Ask".to_string()));
        assert_eq!(access[1..].iter().map(|(v, _)| v.as_str()).collect::<Vec<_>>(), ["settings", "ask", "edits", "auto", "full"]);
    }
}
