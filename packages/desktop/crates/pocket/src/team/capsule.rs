//! The island over a lead's chat: the capsule with a ring per task, the alert when a worker needs you, and the list it opens.

use super::progress::{self, Click, Cta};
use super::ring::ring;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::{Decision, Summary, TeamTask};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

/// Dark in either appearance, as the design's island is. GPUI can't blur what's behind it, so it's nearly opaque.
pub(super) const FILL: Token = Token::fixed(0x16161af2);
pub(super) const INK: Token = Token::fixed(0xffffffeb);
pub(super) const DIM: Token = Token::fixed(0xffffff8c);
const CTA_FILL: Token = Token::fixed(0xffffff24);
const CTA_HOVER: Token = Token::fixed(0xffffff38);
pub(super) const ON_LIGHT: Token = Token::fixed(0x111113ff);
pub(super) const LIGHT_HOVER: Token = Token::fixed(0xe9e9ecff);

pub(super) fn island_shadow() -> Vec<BoxShadow> {
    vec![ui::ring(Token::fixed(0xffffff14), 0.5), ui::shadow(rgba(0x11111347), 12., 32.), ui::shadow(rgba(0x11111329), 2., 8.)]
}

/// The capsule's dark pill.
pub(super) fn pill() -> Div {
    div().h(px(38.)).pl(px(12.)).pr(px(6.)).flex().flex_none().items_center().gap(px(10.)).rounded(px(19.)).bg(FILL).shadow(island_shadow()).text_size(px(12.5)).text_color(INK).whitespace_nowrap().occlude()
}

/// The capsule's button, white when it's what to do next.
pub(super) fn cta(key: String, label: &'static str, light: bool) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(26.))
        .px(px(11.))
        .flex()
        .items_center()
        .rounded(px(13.))
        .cursor_pointer()
        .font_weight(FontWeight::SEMIBOLD)
        .when(light, |d| d.bg(WHITE).text_color(ON_LIGHT).hover(|s| s.bg(LIGHT_HOVER)))
        .when(!light, |d| d.bg(CTA_FILL).text_color(WHITE).hover(|s| s.bg(CTA_HOVER)))
        .child(label)
}

/// Hangs `stack`, its first element lowest, 12px above the top of the composer column it leads, centred over the chat.
pub(super) fn hang(stack: Vec<AnyElement>) -> AnyElement {
    let shown = div().absolute().bottom_full().left_0().right_0().pb(px(12.)).flex().flex_col_reverse().items_center().gap(px(10.)).children(stack);
    div().relative().w_full().h_0().child(shown).into_any_element()
}

/// Fades `e` in as it rises 6px into place.
pub(super) fn rise<E: Styled + IntoElement + 'static>(key: String, e: E, reduce_motion: bool) -> AnyElement {
    if reduce_motion {
        return e.into_any_element();
    }
    e.with_animation(id(key), Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()), |e, t| e.opacity(t).mb(px(-6. * (1. - t)))).into_any_element()
}

/// Amber on the dark capsule.
const WARN: Token = WAITING_DOT;

/// The alert's buttons.
fn dark_button(key: String, label: &'static str, light: bool) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(26.))
        .px(px(10.))
        .flex()
        .items_center()
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .when(light, |d| d.bg(WHITE).text_color(ON_LIGHT).hover(|s| s.bg(LIGHT_HOVER)))
        .when(!light, |d| d.text_color(Token::fixed(0xffffffbf)).hover(|s| s.bg(Token::fixed(0xffffff1a)).text_color(WHITE)))
        .child(label)
}

impl Desktop {
    /// Over the lead's chat: the alert for a worker that needs you, else the capsule and the list it opens.
    pub(crate) fn team_island(&self, agent: &Summary, cx: &mut Context<Self>) -> Option<AnyElement> {
        let team = self.agents.team(&agent.id)?;
        let lead = agent.id.clone();
        let reduce = cx.reduce_motion();
        let rings = |key: &str| div().flex().gap(px(4.)).children(team.tasks.iter().map(|t| ring(id(format!("team-ring:{key}:{lead}:{}", t.id)), progress::mark(&t.state), reduce)));
        if let Some(task) = progress::needing(team) {
            return Some(hang(vec![rise(format!("team-alert-in:{lead}:{}", task.id), self.needs_alert(task, rings("alert"), cx), reduce)]));
        }
        let (working, into) = (agent.status == "working", self.lead_branch(&lead));
        let kept = self.team.island.edits.get(&lead).map(|e| e.picks().len());
        let c = progress::capsule(team, kept, working, &into, crate::util::now_ms())?;
        let label = div()
            .flex()
            .gap(px(4.))
            .child(div().font_weight(FontWeight::MEDIUM).text_color(if c.warn { WARN } else { INK }).child(c.label))
            .when(!c.detail.is_empty(), |d| d.child(div().text_color(DIM).child(c.detail)));
        let (click, at) = (c.click, lead.clone());
        let body = div()
            .id(id(format!("team-capsule:{lead}")))
            .flex()
            .items_center()
            .gap(px(10.))
            .cursor_pointer()
            .children(c.rings.then(|| rings("capsule")))
            .children((!c.rings).then(|| icon("agent-map", 14., INK)))
            .child(label)
            // A press, not a click: it runs before the list's press-outside, which would shut the list this toggles.
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                match click {
                    Click::Sheet => this.team_review(&at, window, cx),
                    Click::List => this.team_toggle_list(&at, cx),
                }
            }));
        let action = c.cta.map(|cta| self.capsule_cta(&lead, cta, cx));
        let capsule = pill().when(action.is_none(), |d| d.pr(px(14.))).child(body).children(action);
        let list = (self.team.island.list.as_deref() == Some(lead.as_str())).then(|| ui::menu_in(id(format!("team-list-in:{lead}")), self.team_list(team, working, &into, cx)).into_any_element());
        Some(hang(std::iter::once(rise(format!("team-capsule-in:{lead}"), capsule, reduce)).chain(list).collect()))
    }

    fn capsule_cta(&self, lead: &str, c: Cta, cx: &mut Context<Self>) -> Stateful<Div> {
        let lead = lead.to_string();
        let (label, light) = match c {
            Cta::Review => ("Review", false),
            Cta::Merge { ready } => ("Merge", ready),
        };
        cta(format!("team-cta:{lead}"), label, light).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            match c {
                Cta::Review => this.team_review(&lead, window, cx),
                Cta::Merge { .. } => this.team_merge(&lead, window, cx),
            }
        }))
    }

    /// The capsule grown into what the worker asks; Open goes to its chat.
    fn needs_alert(&self, task: &TeamTask, rings: Div, cx: &mut Context<Self>) -> Stateful<Div> {
        let ask = self.agents.pending.iter().find(|p| p.agent_id == task.worker);
        let who = self.agent_name(&task.agent);
        let (title, command) = match ask {
            Some(p) => {
                let (verb, what) = p.parts();
                (format!("{who} {}", verb.to_lowercase()), Some(what))
            }
            None => (format!("{who} needs you"), None),
        };
        let worker = task.worker.clone();
        let open = dark_button(format!("team-open:{}", task.id), "Open", false).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.team_open_worker(&worker, window, cx)));
        let answers = ask.filter(|p| p.questions().is_empty() && !self.agents.observe_only()).map(|p| {
            let (deny, allow) = (p.request_id.clone(), p.request_id.clone());
            [
                dark_button(format!("team-deny:{deny}"), "Deny", false).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_answer(&deny, Decision::Deny, cx))),
                dark_button(format!("team-allow:{allow}"), "Allow once", true).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_answer(&allow, Decision::Allow, cx))),
            ]
        });
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .text_color(DIM)
            .child(rings)
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(WARN).child("Needs you"))
            .child(div().min_w_0().truncate().child(format!("· {}", task.title)));
        let well = command.map(|c| div().mt(px(8.)).px(px(10.)).py(px(7.)).rounded(px(8.)).bg(Token::fixed(0xffffff17)).font_family(MONO).text_size(px(12.5)).child(c));
        div()
            .id(id(format!("team-alert:{}", task.id)))
            .w(px(400.))
            .pt(px(14.))
            .pb(px(12.))
            .pl(px(16.))
            .pr(px(14.))
            .rounded(px(22.))
            .bg(FILL)
            .shadow(island_shadow())
            .occlude()
            .text_color(INK)
            .child(head)
            .child(div().mt(px(8.)).text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child(title))
            .children(well)
            .child(div().mt(px(12.)).flex().items_center().gap(px(6.)).child(open).child(div().flex_1()).children(answers.into_iter().flatten()))
    }
}
