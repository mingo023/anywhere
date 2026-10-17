use crate::composer::Menu;
use crate::composer::model_menu::{GAP, PAD, ROW, Side, check, row, rows};
use crate::desktop::Desktop;
use crate::settings::models::{effort_label, listed_models};
use agents::Summary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// The model's short name and its effort, labelled as the new session's pill labels them.
pub(super) fn label(agent: &Summary) -> (String, Option<String>) {
    (agents::model_label(agent), agent.effort.as_deref().filter(|e| !e.is_empty()).map(effort_label))
}

/// Whether the agent runs `value`; Claude Code reports an alias's full id, as "claude-opus-5-5" for "opus".
fn runs(agent: &Summary, value: &str) -> bool {
    agent.model.as_deref().is_some_and(|m| m == value || agent.provider == "claude" && m.split('-').any(|w| w == value))
}

/// The agent's model and effort, as read where they can't be switched.
fn model_pill(agent: &Summary) -> Div {
    let (model, effort) = label(agent);
    div()
        .h(px(30.))
        .max_w(px(248.))
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(7.))
        .pr(px(6.))
        .rounded(px(9.))
        .text_size(px(12.5))
        .child(provider_icon(&agent.provider, 16., TEXT_2))
        .child(div().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(model))
        .children(effort.map(|e| div().flex_none().text_color(TEXT_3).child(e)))
}

fn show(this: &mut Desktop, side: Side, cx: &mut Context<Desktop>) {
    let draft = &mut this.chats.composer.draft;
    if draft.side != Some(side) {
        draft.side = Some(side);
        cx.notify();
    }
}

impl Desktop {
    /// The model pill, a picker of model and effort once pocketd can switch them.
    pub(super) fn chat_model_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Div {
        if view_only || !self.agents.settings_offered() {
            return model_pill(agent);
        }
        let open = self.chats.composer.draft.menu == Some(Menu::Model);
        let chip = model_pill(agent)
            .id("chat-model")
            .cursor_pointer()
            .when(open, |d| d.bg(FILL_2))
            .hover(|s| s.bg(FILL_2))
            .child(icon("chevron-down", 12., TEXT_4))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.load_models(cx);
                this.chats.composer.draft.toggle(Menu::Model);
                cx.notify();
            }));
        div().relative().child(chip).children(open.then(|| self.chat_model_menu(agent, cx)))
    }

    /// Two rows, Model and Effort, each opening its own menu beside them; it opens upward, as the composer sits at the bottom.
    fn chat_model_menu(&self, agent: &Summary, cx: &mut Context<Self>) -> Div {
        let side = self.chats.composer.draft.side;
        let levels = self.efforts_on(&agent.provider, agent.model.as_deref().unwrap_or_default());
        let side_row = |id: &'static str, to: Side, title: &'static str, value: String| {
            row(id, side == Some(to))
                .child(div().flex_1().font_weight(FontWeight::MEDIUM).child(title))
                .child(div().min_w_0().truncate().text_color(TEXT_2).child(value))
                .child(icon("chevron-right", 12., TEXT_4))
                .on_hover(cx.listener(move |this, hovered: &bool, _, cx| if *hovered { show(this, to, cx) }))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| show(this, to, cx)))
        };
        let model_row = side_row("chat-side-model", Side::Model, "Model", label(agent).0);
        let effort_row = if levels.is_empty() {
            row("chat-side-effort", false).cursor_default().child(div().flex_1().font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child("Effort")).child(div().text_color(TEXT_4).child(format!("Set by {}", self.agent_name(&agent.provider))))
        } else {
            side_row("chat-side-effort", Side::Effort, "Effort", agent.effort.as_deref().filter(|e| !e.is_empty()).map_or_else(|| "Default".into(), effort_label))
        };
        let main = ui::pop(div()).w(px(250.)).flex_none().p(px(PAD)).child(rows("chat-sides", vec![model_row, effort_row]));

        let side_menu = side.map(|s| {
            let choices = match s {
                Side::Model => listed_models(&agent.provider, self.codex_models()),
                Side::Effort => levels.iter().map(|e| (e.clone(), effort_label(e))).collect(),
            };
            let list = choices
                .into_iter()
                .enumerate()
                .map(|(i, (value, name))| {
                    let on = match s {
                        Side::Model => runs(agent, &value),
                        Side::Effort => agent.effort.as_ref() == Some(&value),
                    };
                    let id = agent.id.clone();
                    row(("chat-switch-row", i), on).child(div().flex_1().min_w_0().truncate().font_weight(FontWeight::MEDIUM).child(name)).child(check(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let (model, effort) = if s == Side::Model { (value.as_str(), "") } else { ("", value.as_str()) };
                        this.outbox.settings(&id, model, effort);
                        this.chats.composer.draft.menu = None;
                        cx.notify();
                    }))
                })
                .collect();
            // Bottoms align, so lifting the Model side by a row sets it beside its own row.
            let lift = if s == Side::Model { ROW + GAP } else { 0. };
            ui::pop(div()).w(px(220.)).mb(px(lift)).p(px(PAD)).child(rows("chat-switches", list))
        });

        let menus = div()
            .id("chat-model-menu")
            .flex()
            .items_end()
            .gap(px(6.))
            // Menus hang over the transcript; without this a click on one also lands on what it covers.
            .occlude()
            .children(side_menu)
            .child(main)
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.chats.composer.draft.menu = None;
                cx.notify();
            }));
        div().absolute().top(px(-6.)).right_0().child(deferred(anchored().anchor(Anchor::BottomRight).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-model-menu-in", menus))).with_priority(1))
    }
}

#[cfg(test)]
mod tests {
    use super::{Summary, label, runs};

    #[test]
    fn the_model_pill_reads_model_and_effort() {
        let agent = |model: &str, effort: Option<&str>| Summary { provider: "claude".into(), model: Some(model.into()), effort: effort.map(Into::into), ..Default::default() };
        assert_eq!(label(&agent("claude-opus-4-5", Some("xhigh"))), ("Opus 4.5".into(), Some("Extra high".into())));
        assert_eq!(label(&agent("claude-opus-4-5", Some("medium"))).1.as_deref(), Some("Medium"));
        assert_eq!(label(&agent("claude-opus-4-5", None)).1, None);
    }

    #[test]
    fn a_claude_alias_is_the_model_its_full_id_names() {
        let agent = |provider: &str, model: &str| Summary { provider: provider.into(), model: Some(model.into()), ..Default::default() };
        assert!(runs(&agent("claude", "claude-opus-5-5"), "opus"));
        assert!(runs(&agent("claude", "opus"), "opus"));
        assert!(!runs(&agent("claude", "claude-opus-5-5"), "sonnet"));
        assert!(runs(&agent("codex", "gpt-6-sol"), "gpt-6-sol"));
        assert!(!runs(&agent("codex", "gpt-6-sol"), "sol"));
    }
}
