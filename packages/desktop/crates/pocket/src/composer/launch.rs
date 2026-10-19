use crate::desktop::Desktop;
use crate::settings::models::{model_label, model_options};
use gpui_kit::prelude::FluentBuilder as _;
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
pub(crate) fn access_color(access: &str) -> Token {
    if access == "full" { WAITING_TEXT } else { TEXT }
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

/// What a running agent switches between: each of `ACCESSES` but its own settings, which only a launch reads.
pub(crate) fn switch_choices() -> Vec<(String, String, String)> {
    access_choices("").into_iter().filter(|(v, _, _)| !v.is_empty() && v != "settings").collect()
}

/// A composer chip led by `glyph`; `toggle` opens or closes its menu.
pub(crate) fn chip(id: &'static str, open: bool, glyph: &str, label: impl Into<SharedString>, color: Token, toggle: impl Fn(&mut Desktop, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Stateful<Div> {
    super::picker(super::pill().child(icon(glyph, 14., TEXT_2)).child(div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(color).child(label.into())), id, open)
        .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            toggle(this, cx);
        }))
}

pub(crate) fn access_chip(id: &'static str, open: bool, picked: &str, app: &str, toggle: impl Fn(&mut Desktop, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let access = if picked.is_empty() { app } else { picked };
    chip(id, open, "shield", access_shown(picked, app), access_color(access), toggle, cx)
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

/// A choice titled `label` over its `hint`, after an optional `lead` glyph.
pub(crate) fn access_row(id: impl Into<ElementId>, selected: bool, lead: Option<&str>, label: &str, hint: &str, color: Token) -> Stateful<Div> {
    div()
        .id(id)
        .px(px(8.))
        .py(px(7.))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(9.))
        .rounded(px(6.))
        .cursor_pointer()
        .when(selected, |d| d.bg(FILL_2))
        .when(!selected, |d| d.hover(|s| s.bg(FILL_2)))
        .children(lead.map(|glyph| div().self_start().pt(px(1.)).child(icon(glyph, 14., TEXT_2))))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(color).child(label.to_string()))
                .child(div().text_size(px(11.5)).text_color(TEXT_3).child(hint.to_string())),
        )
        .child(div().w(px(16.)).flex().flex_none().justify_end().when(selected, |d| d.child(icon("check", 14., TEXT))))
}

#[cfg(test)]
mod tests {
    use super::{access_choices, access_shown, model_choices, switch_choices};

    #[test]
    fn a_chip_names_the_pick_else_what_settings_start_with() {
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

    #[test]
    fn a_running_agent_switches_between_ask_edits_auto_and_full() {
        assert_eq!(switch_choices().into_iter().map(|(v, l, _)| (v, l)).collect::<Vec<_>>(), [("ask", "Ask"), ("edits", "Edits"), ("auto", "Auto"), ("full", "Full")].map(|(v, l)| (v.to_string(), l.to_string())));
    }
}
