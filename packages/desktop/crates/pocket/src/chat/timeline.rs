use super::sticky;
use crate::composer::{Sent, split_attachments};
use crate::desktop::Desktop;
use chat::Row;
use gpui_kit::*;
use std::rc::Rc;
use std::time::{Duration, Instant};
use theme::*;

/// The most room between two ticks; more prompts than fit pack closer, down to `MIN_PITCH`.
const PITCH: f32 = 10.;
/// Past this many prompts per pixel, ticks stand for evenly spread prompts rather than one each.
const MIN_PITCH: f32 = 3.;
/// Room kept over and under the rail, so it stays clear of the sticky prompt and the jump pill.
const MARGIN: f32 = 80.;
/// Hit area past the first and last tick.
const SLOP: f32 = 12.;
const HIT_WIDTH: f32 = 44.;
const TICK_RIGHT: f32 = 12.;
const TICK: f32 = 2.;
const BASE_W: f32 = 10.;
const ACTIVE_W: f32 = 16.;
/// How much a tick under the pointer grows, and how fast that falls off with distance, as the Dock magnifies.
const MAG: f32 = 10.;
const SIGMA: f32 = 14.;
const CARD_RIGHT: f32 = 48.;
const CARD_MAX_W: f32 = 320.;
const CARD_PAD_X: f32 = 14.;
const CARD_PAD_Y: f32 = 10.;
const CARD_SIZE: f32 = 13.5;
const CARD_LINE: f32 = 19.;
const CARD_LINES: usize = 3;
const CARD_EDGE: f32 = 8.;
const ENTER: Duration = Duration::from_millis(160);
const SLIDE: Duration = Duration::from_millis(140);
/// More than three lines of the card hold; the rest of a long prompt is never shaped.
const LABEL_CHARS: usize = 400;

/// Where the pointer is on the rail, `y` px under its first tick's top, and whether it's pressed there.
#[derive(Default)]
pub(super) struct Pointer {
    y: Option<f32>,
    pressed: bool,
    /// When the pointer last arrived on the rail: the card grows out of it.
    entered: Option<Instant>,
}

impl Pointer {
    /// Off the rail nothing is hovered or held; whether anything was.
    fn leave(&mut self) -> bool {
        let was = self.y.is_some() || self.pressed;
        *self = Self::default();
        was
    }

    /// Whether a held pointer moving to `y` reached another tick, which scrubs the chat to it.
    fn moved(&mut self, y: f32, rail: &Rail) -> bool {
        let was = self.y.map(|y| rail.tick_at(y));
        self.y = Some(y);
        self.entered.get_or_insert_with(Instant::now);
        self.pressed && was != Some(rail.tick_at(y))
    }

    /// Whether it was held, so the chat lands on the tick it's let go over.
    fn release(&mut self) -> bool {
        std::mem::take(&mut self.pressed)
    }
}

/// The long tick sliding to the prompt being read, from tick position `from` to tick `to` on a rail of `ticks`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Lit {
    from: f32,
    to: usize,
    ticks: usize,
    at: Instant,
}

impl Lit {
    /// Heads for tick `to` from wherever it is now; with nothing to slide from, a rail laid out anew, or `snap`, it's
    /// there at once.
    fn toward(lit: Option<Self>, to: usize, ticks: usize, now: Instant, snap: bool) -> Self {
        match lit {
            Some(l) if l.ticks == ticks && l.to == to => l,
            Some(l) if l.ticks == ticks && !snap => Self { from: l.pos(now), to, ticks, at: now },
            _ => Self { from: to as f32, to, ticks, at: now },
        }
    }

    /// Its tick position at `now`.
    fn pos(&self, now: Instant) -> f32 {
        let t = (now.saturating_duration_since(self.at).as_secs_f32() / SLIDE.as_secs_f32()).min(1.);
        self.from + (self.to as f32 - self.from) * ease_out_quint()(t)
    }

    fn moving(&self, now: Instant) -> bool {
        self.from != self.to as f32 && now.saturating_duration_since(self.at) < SLIDE
    }
}

/// How lit tick `tick` is with the long tick at `pos`: 1 under it, fading to 0 a tick away.
fn glow(pos: f32, tick: usize) -> f32 {
    (1. - (pos - tick as f32).abs()).clamp(0., 1.)
}

/// The pointer on the rail: a press glides the chat to its prompt, a drag follows the pointer at once, and letting go
/// lights the prompt.
#[derive(Clone, Copy)]
enum Gesture {
    Press,
    Drag,
    Release,
}

/// The rail's ticks laid out for `prompts` prompts in a viewport: each tick stands for one prompt, or for evenly
/// spread ones when there are too many to fit.
#[derive(Clone, Debug, PartialEq)]
struct Rail {
    pitch: f32,
    ticks: usize,
    prompts: usize,
}

impl Rail {
    /// None under two prompts, where there's nothing to move between.
    fn new(prompts: usize, viewport: f32) -> Option<Self> {
        let room = viewport - 2. * MARGIN;
        if prompts < 2 || room < MIN_PITCH * 2. {
            return None;
        }
        let ticks = prompts.min((room / MIN_PITCH) as usize);
        Some(Self { pitch: (room / ticks as f32).min(PITCH), ticks, prompts })
    }

    fn height(&self) -> f32 {
        self.pitch * self.ticks as f32
    }

    /// Tick `tick`'s centre, under the rail's top.
    fn y(&self, tick: usize) -> f32 {
        self.pitch * (tick as f32 + 0.5)
    }

    /// The tick nearest `y`, held to the rail.
    fn tick_at(&self, y: f32) -> usize {
        ((y / self.pitch).floor().max(0.) as usize).min(self.ticks - 1)
    }

    /// The prompt tick `tick` stands for.
    fn prompt(&self, tick: usize) -> usize {
        if self.ticks == self.prompts { tick } else { (tick * (self.prompts - 1) + (self.ticks - 1) / 2) / (self.ticks - 1) }
    }

    /// The tick standing for prompt `prompt`.
    fn tick_of(&self, prompt: usize) -> usize {
        if self.ticks == self.prompts { prompt } else { (prompt * (self.ticks - 1) + (self.prompts - 1) / 2) / (self.prompts - 1) }
    }
}

fn prompt_rows(rows: &[Row]) -> Vec<usize> {
    rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Prompt { .. })).map(|(ix, _)| ix).collect()
}

/// Which of `prompts` is being read: the last that starts at or above `top`, the row at the list's top; at the list's
/// end the last, whose reply may be too short to reach the top.
fn current(prompts: &[usize], top: usize, at_end: bool) -> usize {
    if at_end {
        return prompts.len().saturating_sub(1);
    }
    prompts.iter().rposition(|&ix| ix <= top).unwrap_or(0)
}

/// A tick's width: longer by its `glow` from the prompt being read, and grown as the pointer nears it, `d` px away.
fn width(glow: f32, d: Option<f32>) -> f32 {
    let base = BASE_W + (ACTIVE_W - BASE_W) * glow;
    base + d.map_or(0., |d| MAG * (-(d / SIGMA).powi(2) / 2.).exp())
}

/// Where a card `height` tall sits beside a tick at `y` in a viewport: centred on it, kept inside.
fn card_top(y: f32, height: f32, viewport: f32) -> f32 {
    (y - height / 2.).min(viewport - height - CARD_EDGE).max(CARD_EDGE)
}

/// A prompt as the card reads it: its text on one line, else what it attached.
fn label(text: &str) -> String {
    let Sent { text, files, images, elements } = split_attachments(text);
    let text: String = text.chars().take(LABEL_CHARS).collect();
    match sticky::one_line(&text) {
        t if t.is_empty() => sticky::attached_count(images.len(), files.len(), elements.len()),
        t => t,
    }
}

impl Desktop {
    /// A tick per prompt down the chat's right edge, the one being read longer. Hovering shows the prompt beside it;
    /// a click, or a drag along the ticks, scrolls to it. The pointer's moves notify only the transcript's region.
    pub(super) fn chat_timeline(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<Div> {
        let (now, snap) = (Instant::now(), cx.reduce_motion() || self.capturing);
        let view = self.chats.views.get_mut(agent)?;
        let s = &view.session;
        let prompts = prompt_rows(&s.rows);
        let viewport = view.list.viewport_bounds();
        let height = f32::from(viewport.size.height) - view.inset;
        let rail = Rail::new(prompts.len(), height)?;
        let top = view.inset + (height - rail.height()) / 2.;
        let origin = f32::from(viewport.top()) + top;
        let active = rail.tick_of(current(&prompts, view.top_row().0, view.distance() <= 2.));
        let lit = Lit::toward(view.lit, active, rail.ticks, now, snap);
        view.lit = Some(lit);
        if lit.moving(now) {
            window.request_animation_frame();
        }
        let pos = lit.pos(now);
        let pointer = view.pointer.read(cx);
        let (y, entered) = (pointer.y, pointer.entered);
        let hovered = y.map(|y| rail.tick_at(y));

        let ticks = (0..rail.ticks).map(|tick| {
            let glow = glow(pos, tick);
            let colour = if hovered == Some(tick) { Hsla::from(TEXT) } else { Hsla::from(TEXT_6).blend(Hsla::from(TEXT_3).opacity(glow)) };
            div().absolute().top(px(rail.y(tick) - TICK / 2.)).right(px(TICK_RIGHT)).h(px(TICK)).w(px(width(glow, y.map(|y| y - rail.y(tick))))).rounded(px(TICK / 2.)).bg(colour)
        });

        let at = move |e: Point<Pixels>| f32::from(e.y) - origin;
        let jump = {
            let (desktop, agent, prompts, rail) = (cx.weak_entity(), agent.to_string(), prompts.clone(), rail.clone());
            Rc::new(move |tick: usize, gesture: Gesture, window: &mut Window, cx: &mut App| {
                let ix = prompts[rail.prompt(tick)];
                desktop
                    .update(cx, |d, cx| match gesture {
                        Gesture::Press => d.chat_seek_prompt(&agent, ix, window, cx),
                        Gesture::Drag => d.chat_jump_to_prompt(&agent, ix, cx),
                        Gesture::Release => d.chat_land_on_prompt(&agent, ix, cx),
                    })
                    .ok();
            })
        };
        let p = view.pointer.clone();
        let hit = div()
            .id("chat-timeline")
            .absolute()
            .top(px(top - SLOP))
            .right_0()
            .w(px(HIT_WIDTH))
            .h(px(rail.height() + 2. * SLOP))
            // Over the transcript's right edge in a narrow chat: a click here is the rail's alone.
            .block_mouse_except_scroll()
            .on_hover({
                let p = p.clone();
                move |over, _, cx| {
                    if !over {
                        p.update(cx, |p, cx| {
                            if p.leave() {
                                cx.notify();
                            }
                        })
                    }
                }
            })
            .on_mouse_move({
                let (p, jump, rail) = (p.clone(), jump.clone(), rail.clone());
                move |e: &MouseMoveEvent, window, cx| {
                    let y = at(e.position);
                    let scrub = p.update(cx, |p, cx| {
                        cx.notify();
                        p.moved(y, &rail)
                    });
                    if scrub {
                        jump(rail.tick_at(y), Gesture::Drag, window, cx);
                    }
                }
            })
            .on_mouse_down(MouseButton::Left, {
                let (p, jump, rail) = (p.clone(), jump.clone(), rail.clone());
                move |e: &MouseDownEvent, window, cx| {
                    p.update(cx, |p, _| p.pressed = true);
                    jump(rail.tick_at(at(e.position)), Gesture::Press, window, cx);
                }
            })
            .on_mouse_up(MouseButton::Left, {
                let (p, rail) = (p.clone(), rail.clone());
                move |e: &MouseUpEvent, window, cx| {
                    if p.update(cx, |p, _| p.release()) {
                        jump(rail.tick_at(at(e.position)), Gesture::Release, window, cx);
                    }
                }
            });

        let card = hovered.and_then(|tick| {
            let Row::Prompt { turn } = s.rows[prompts[rail.prompt(tick)]] else { return None };
            let text = label(s.transcript.turns.get(turn)?.prompt.as_ref()?.text.as_str());
            if text.is_empty() {
                return None;
            }
            let lines = super::user_bubble::text_lines(&text, CARD_SIZE, CARD_MAX_W - 2. * CARD_PAD_X, window).min(CARD_LINES);
            let card_h = lines as f32 * CARD_LINE + 2. * CARD_PAD_Y;
            let card = ui::pop(div())
                .absolute()
                .top(px(card_top(top + rail.y(tick), card_h, height)))
                .right(px(CARD_RIGHT))
                .max_w(px(CARD_MAX_W))
                .px(px(CARD_PAD_X))
                .py(px(CARD_PAD_Y))
                .text_size(px(CARD_SIZE))
                .line_height(px(CARD_LINE))
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT)
                .line_clamp(CARD_LINES)
                .child(text);
            Some(match entered.filter(|at| at.elapsed() < ENTER && !cx.reduce_motion()) {
                Some(_) => {
                    let enter = Animation::new(ENTER).with_easing(ease_out_quint());
                    card.with_animation("chat-timeline-in", enter, |d, t| d.opacity(t).mr(px(-6. * (1. - t)))).into_any_element()
                }
                None => card.into_any_element(),
            })
        });

        let rail = div().absolute().top(px(top)).right_0().w(px(HIT_WIDTH)).h(px(rail.height())).children(ticks);
        Some(div().absolute().inset_0().child(rail).child(hit).children(card))
    }
}

#[cfg(test)]
mod tests {
    use super::{CARD_EDGE, Lit, PITCH, Pointer, Rail, SLIDE, card_top, current, glow, label, prompt_rows, width};
    use chat::Row;
    use std::time::{Duration, Instant};

    #[test]
    fn a_rail_needs_two_prompts_to_move_between() {
        assert_eq!(Rail::new(1, 800.), None);
        assert!(Rail::new(2, 800.).is_some());
    }

    #[test]
    fn ticks_sit_ten_px_apart_until_the_prompts_outgrow_the_room() {
        assert_eq!(Rail::new(12, 800.).unwrap().pitch, PITCH);
        let packed = Rail::new(150, 800.).unwrap();
        assert_eq!((packed.ticks, packed.pitch), (150, 640. / 150.));
    }

    #[test]
    fn past_one_tick_per_three_px_a_tick_stands_for_spread_out_prompts() {
        let rail = Rail::new(1000, 400.).unwrap();
        assert_eq!(rail.ticks, 80);
        assert_eq!((rail.prompt(0), rail.prompt(79)), (0, 999), "the first and last prompts keep their ticks");
        assert_eq!(rail.tick_of(999), 79);
        assert_eq!(rail.tick_of(rail.prompt(40)), 40);
    }

    #[test]
    fn the_pointer_picks_the_tick_under_it_held_to_the_rail() {
        let rail = Rail::new(5, 800.).unwrap();
        assert_eq!(rail.tick_at(-12.), 0);
        assert_eq!(rail.tick_at(25.), 2);
        assert_eq!(rail.tick_at(500.), 4);
        assert_eq!(rail.y(2), 25.);
    }

    #[test]
    fn the_prompt_being_read_is_the_last_one_at_or_above_the_top() {
        let rows = [Row::Working, Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::Prompt { turn: 1 }, Row::Live { turn: 1 }, Row::Pending(0)];
        let p = prompt_rows(&rows);
        assert_eq!(p, [1, 3], "a send not yet echoed has no tick");
        assert_eq!(current(&p, 0, false), 0, "above the first prompt it's the first");
        assert_eq!(current(&p, 2, false), 0);
        assert_eq!(current(&p, 4, false), 1);
        assert_eq!(current(&p, 1, true), 1, "at the end it's the last, however short its reply");
    }

    #[test]
    fn a_tick_grows_as_the_pointer_nears_it() {
        assert_eq!(width(0., None), 10.);
        assert_eq!(width(1., None), 16.);
        assert_eq!(width(0., Some(0.)), 20.);
        assert!(width(0., Some(14.)) < width(0., Some(4.)));
        assert!(width(0., Some(80.)) - 10. < 0.01);
    }

    #[test]
    fn the_long_tick_slides_to_the_next_prompt_and_settles_there() {
        let now = Instant::now();
        let lit = Lit::toward(Some(Lit::toward(None, 3, 10, now, false)), 4, 10, now, false);
        assert!(lit.moving(now) && lit.pos(now) == 3.);
        let mid = lit.pos(now + SLIDE / 2);
        assert!(mid > 3.5 && mid < 4., "eases out: most of the way by halfway");
        assert!(!lit.moving(now + SLIDE) && lit.pos(now + SLIDE) == 4.);
        assert_eq!((glow(3.5, 3), glow(3.5, 4), glow(3.5, 5)), (0.5, 0.5, 0.));
    }

    #[test]
    fn a_new_prompt_mid_slide_turns_the_long_tick_from_where_it_is() {
        let now = Instant::now();
        let lit = Lit::toward(Some(Lit::toward(None, 0, 10, now, false)), 5, 10, now, false);
        let later = now + Duration::from_millis(30);
        let turned = Lit::toward(Some(lit), 1, 10, later, false);
        assert_eq!(turned.pos(later), lit.pos(later));
        assert_eq!(Lit::toward(Some(turned), 1, 10, later + SLIDE / 2, false), turned, "the same prompt keeps its slide");
    }

    #[test]
    fn the_long_tick_jumps_without_motion_or_on_a_rail_laid_out_anew() {
        let now = Instant::now();
        let lit = Lit::toward(None, 2, 10, now, false);
        assert!(!lit.moving(now), "nothing to slide from at first");
        assert_eq!(Lit::toward(Some(lit), 6, 10, now, true).pos(now), 6., "reduced motion");
        assert_eq!(Lit::toward(Some(lit), 6, 11, now, false).pos(now), 6., "another prompt's tick moved the rest");
    }

    #[test]
    fn the_card_centres_on_its_tick_but_stays_in_the_viewport() {
        assert_eq!(card_top(300., 40., 800.), 280.);
        assert_eq!(card_top(5., 40., 800.), CARD_EDGE);
        assert_eq!(card_top(795., 40., 800.), 800. - 40. - CARD_EDGE);
    }

    #[test]
    fn a_held_pointer_scrubs_only_when_it_reaches_another_tick() {
        let rail = Rail::new(5, 800.).unwrap();
        let mut p = Pointer::default();
        assert!(!p.moved(5., &rail), "hovering doesn't scroll");
        p.pressed = true;
        assert!(!p.moved(8., &rail), "still on the first tick");
        assert!(p.moved(25., &rail));
        assert!(p.release() && !p.release(), "letting go lands once");
    }

    #[test]
    fn leaving_the_rail_lets_go_of_what_it_held() {
        let rail = Rail::new(5, 800.).unwrap();
        let mut p = Pointer::default();
        assert!(!p.leave(), "nothing to redraw");
        p.pressed = true;
        p.moved(25., &rail);
        assert!(p.leave());
        assert!(!p.release() && !p.moved(40., &rail), "a press outside the rail doesn't scrub");
    }

    #[test]
    fn the_card_reads_a_prompt_on_one_line_and_no_further_than_it_shows() {
        assert_eq!(label("Fix these:\n1) filter\n\n  2) sort "), "Fix these: 1) filter 2) sort");
        assert_eq!(label(&"word ".repeat(200)).len(), 399);
        assert_eq!(label("  \n "), "");
    }
}
