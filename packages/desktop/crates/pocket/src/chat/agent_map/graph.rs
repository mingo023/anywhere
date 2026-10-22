use super::{CARD, calls, main_line, mark, meta, name};
use crate::chat::sub_agent::{self, Mark};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::Summary;
use chat::{Spawned, ToolStatus, Transcript};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

const W: f32 = 232.;
const H: f32 = 64.;
/// Between a parent's column and its children's, where the edges bend.
const GAP: f32 = 44.;
const VGAP: f32 = 10.;
const PAD: f32 = 16.;
/// How far a node slides in from the left as it appears.
const ENTER: f32 = 10.;
const DASH: f32 = 3.;
const DASH_GAP: f32 = 5.;
const CARD_HOVER: Token = Token::new(0xfafafaff, 0x3d3d3dff);

type Pt = (f32, f32);

/// Each node's top left and the graph's size. Node 0 is the main agent and node `i + 1` is the sub-agent whose parent
/// is `parents[i]`, an index into the same list, or the main agent. One column per generation; a parent centres on its
/// children, and the leaves stack in order.
fn layout(parents: &[Option<usize>]) -> (Vec<Pt>, Pt) {
    let n = parents.len() + 1;
    let mut kids = vec![Vec::new(); n];
    let mut depth = vec![0; n];
    for (i, p) in parents.iter().enumerate() {
        let p = p.map_or(0, |p| p + 1);
        kids[p].push(i + 1);
        depth[i + 1] = depth[p] + 1;
    }
    fn place(node: usize, kids: &[Vec<usize>], depth: &[usize], slot: &mut usize, at: &mut [Pt]) {
        let x = PAD + depth[node] as f32 * (W + GAP);
        let y = match kids[node].as_slice() {
            [] => {
                *slot += 1;
                PAD + (*slot - 1) as f32 * (H + VGAP)
            }
            all => {
                all.iter().for_each(|&k| place(k, kids, depth, slot, at));
                (at[all[0]].1 + at[all[all.len() - 1]].1) / 2.
            }
        };
        at[node] = (x, y);
    }
    let (mut at, mut slot) = (vec![(0., 0.); n], 0);
    place(0, &kids, &depth, &mut slot, &mut at);
    let columns = depth.iter().max().map_or(1, |d| d + 1) as f32;
    (at, (PAD * 2. + columns * W + (columns - 1.) * GAP, PAD * 2. + slot.max(1) as f32 * (H + VGAP) - VGAP))
}

/// The edge from a parent's right side to a child's left: across, down the middle of the gap with rounded bends, and
/// across again, as points close enough to draw the bends as lines.
fn elbow(from: Pt, to: Pt) -> Vec<Pt> {
    let dy = to.1 - from.1;
    if dy.abs() < 0.5 {
        return vec![from, to];
    }
    let mid = from.0 + GAP / 2.;
    let r = 8f32.min(dy.abs() / 2.);
    let dir = dy.signum();
    let bend = |a: Pt, corner: Pt, b: Pt| (1..=4).map(move |i| {
        let t = i as f32 / 4.;
        let u = 1. - t;
        (u * u * a.0 + 2. * u * t * corner.0 + t * t * b.0, u * u * a.1 + 2. * u * t * corner.1 + t * t * b.1)
    });
    let mut points = vec![from, (mid - r, from.1)];
    points.extend(bend((mid - r, from.1), (mid, from.1), (mid, from.1 + dir * r)));
    points.push((mid, to.1 - dir * r));
    points.extend(bend((mid, to.1 - dir * r), (mid, to.1), (mid + r, to.1)));
    points.push(to);
    points
}

/// The stretch of `points` from distance `a` to `b` along it.
fn between(points: &[Pt], a: f32, b: f32) -> Vec<Pt> {
    let mut out = Vec::new();
    let mut walked = 0.;
    for w in points.windows(2) {
        let (p, q) = (w[0], w[1]);
        let len = ((q.0 - p.0).powi(2) + (q.1 - p.1).powi(2)).sqrt();
        let at = |d: f32| {
            let t = if len == 0. { 0. } else { ((d - walked) / len).clamp(0., 1.) };
            (p.0 + (q.0 - p.0) * t, p.1 + (q.1 - p.1) * t)
        };
        if walked + len >= a && walked <= b {
            if out.is_empty() {
                out.push(at(a));
            }
            out.push(at(b.min(walked + len)));
        }
        walked += len;
    }
    out
}

/// `points` cut into dashes `on` long and `off` apart, the pattern moved `phase` along so it flows from parent to child.
fn dashes(points: &[Pt], on: f32, off: f32, phase: f32) -> Vec<Vec<Pt>> {
    let total: f32 = points.windows(2).map(|w| ((w[1].0 - w[0].0).powi(2) + (w[1].1 - w[0].1).powi(2)).sqrt()).sum();
    let period = on + off;
    let mut start = phase.rem_euclid(period) - period;
    let mut out = Vec::new();
    while start < total {
        let (a, b) = (start.max(0.), (start + on).min(total));
        if b > a {
            out.push(between(points, a, b));
        }
        start += period;
    }
    out
}

#[derive(Clone)]
struct Edge {
    points: Vec<Pt>,
    status: ToolStatus,
}

fn paint(edges: &[Edge], phase: f32, bounds: Bounds<Pixels>, window: &mut Window) {
    let at = |(x, y): Pt| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
    for e in edges {
        let (pieces, color): (Vec<Vec<Pt>>, Hsla) = match e.status {
            ToolStatus::Running => (dashes(&e.points, DASH, DASH_GAP, phase), Hsla::from(ACCENT).opacity(0.6)),
            ToolStatus::Failed => (vec![e.points.clone()], Hsla::from(FAILED).opacity(0.45)),
            ToolStatus::Ok => (vec![e.points.clone()], SEPARATOR_STRONG.into()),
        };
        let mut path = PathBuilder::stroke(px(1.25));
        for piece in pieces.iter().filter(|p| p.len() > 1) {
            path.move_to(at(piece[0]));
            piece[1..].iter().for_each(|&p| path.line_to(at(p)));
        }
        if let Ok(path) = path.build() {
            window.paint_path(path, color);
        }
    }
}

fn edges_layer(edges: Vec<Edge>, phase: f32, size: Pt) -> Canvas<()> {
    canvas(|_, _, _| (), move |bounds, _, window, _| paint(&edges, phase, bounds, window)).absolute().top_0().left_0().w(px(size.0)).h(px(size.1))
}

/// A node's card: its mark and name, then what it took, then what it's on.
fn card(key: String, (x, y): Pt, selected: bool, failed: bool) -> Stateful<Div> {
    div()
        .id(id(key))
        .absolute()
        .top(px(y))
        .left(px(x))
        .w(px(W))
        .h(px(H))
        .px(px(12.))
        .flex()
        .flex_col()
        .justify_center()
        .gap(px(2.))
        .rounded(px(12.))
        .bg(CARD)
        .shadow(if selected {
            vec![ui::ring(ACCENT, 1.5), ui::ring(ACCENT_RING, 3.), ui::shadow(rgba(0x11111314), 6., 18.)]
        } else {
            vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x1111130d), 1., 2.), ui::shadow(rgba(0x1111130d), 4., 14.)]
        })
        .when(failed, |d| d.text_color(FAILED))
}

fn title(glyph: impl IntoElement, text: String, color: Token) -> Div {
    div().min_w_0().flex().items_center().gap(px(7.)).child(glyph).child(div().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(color).child(text))
}

fn sub(text: String) -> Div {
    div().pl(px(21.)).min_w_0().truncate().text_size(px(11.5)).text_color(TEXT_3).child(text)
}

/// The third line: a step in the code font, or a state in words.
fn line(text: String, code: bool, color: Token) -> Div {
    let d = div().pl(px(21.)).min_w_0().truncate().text_color(color);
    if code { d.font_family(code_font()).text_size(px(11.)) } else { d.text_size(px(11.5)) }.child(text)
}

impl Desktop {
    /// The graph of who spawned whom and its height; `live` takes clicks, as the graph does while it's the page showing.
    pub(super) fn chat_map_graph(&self, agent: &Summary, t: &Transcript, map: &[Spawned], live: bool, now: i64, cx: &mut Context<Self>) -> (AnyElement, f32) {
        let parents: Vec<_> = map.iter().map(|s| s.parent).collect();
        let (at, size) = layout(&parents);
        let selected = self.chats.map.nav.selected;
        let center = |(x, y): Pt, right: bool| (if right { x + W } else { x }, y + H / 2.);
        let edges: Vec<Edge> = map.iter().enumerate().map(|(i, s)| Edge { points: elbow(center(at[s.parent.map_or(0, |p| p + 1)], true), center(at[i + 1], false)), status: s.call.status }).collect();
        let flowing = edges.iter().any(|e| e.status == ToolStatus::Running);
        let layer = if flowing {
            let edges = edges.clone();
            div().absolute().top_0().left_0().size_full().with_animation(id(format!("chat-map-flow:{}", agent.id)), Animation::new(Duration::from_millis(900)).repeat().with_max_fps(30.), move |d, p| d.child(edges_layer(edges.clone(), p * (DASH + DASH_GAP), size))).into_any_element()
        } else {
            edges_layer(edges, 0., size).into_any_element()
        };

        let main = card(format!("chat-map-node:{}:main", agent.id), at[0], selected == 0, false)
            .child(title(provider_icon(&agent.provider, 14., TEXT_2), "Main agent".into(), TEXT))
            .child(sub(match agent.tokens_used {
                0 => agents::model_label(agent),
                n => format!("{} · {:.1}k context", agents::model_label(agent), n as f64 / 1_000.),
            }))
            .child(line(main_line(t, map), false, TEXT_4));
        let nodes = map.iter().enumerate().map(|(i, s)| {
            let (x, y) = at[i + 1];
            let m = mark(s);
            let failed = m == Mark::Failed;
            let third = match m {
                Mark::Running | Mark::Starting => line(sub_agent::latest(s.agent).map_or_else(|| "Starting…".into(), sub_agent::step), true, TEXT_2),
                Mark::Failed => line(sub_agent::report(s.call, s.agent).and_then(|r| r.lines().next().map(str::to_string)).unwrap_or_else(|| "Failed".into()), true, FAILED),
                Mark::Done => line(format!("Done · {} tool call{}", calls(s.agent), if calls(s.agent) == 1 { "" } else { "s" }), false, TEXT_4),
            };
            let (node, call) = (i + 1, s.call.id.clone());
            card(format!("chat-map-node:{}", s.call.id), (x, y), selected == node, failed)
                .child(title(sub_agent::glyph(&s.call.id, m), name(s), if failed { FAILED } else { TEXT }))
                .child(sub(meta(s.call, s.agent.tokens, now)))
                .child(third)
                .when(live, |d| d.cursor_pointer().hover(|s| s.bg(CARD_HOVER)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_map_show(node, &call, cx))))
                // A node new to the map slides in from its parent's side as it fades in.
                .with_spring(id(format!("chat-map-in:{}", s.call.id)), SpringAnimation::new(SPRING).to(true).from(false), move |d, t| {
                    d.opacity(t.0.clamp(0., 1.)).left(px(x - ENTER * (1. - t.0)))
                })
        });
        let canvas = div().relative().flex_none().m_auto().w(px(size.0)).h(px(size.1)).child(layer).child(main).children(nodes);
        let scroll = div().id(id(format!("chat-map-graph:{}", agent.id))).flex_1().min_h_0().overflow_scroll().flex().child(canvas);
        (scroll.into_any_element(), size.1)
    }
}

#[cfg(test)]
mod tests {
    use super::{GAP, H, PAD, VGAP, W, dashes, elbow, layout};

    #[test]
    fn the_main_agent_centres_on_the_agents_it_spawned_one_column_left() {
        let (at, size) = layout(&[None, None, None]);
        assert_eq!(at[0], (PAD, PAD + (H + VGAP)));
        assert_eq!(at[1..].iter().map(|p| p.0).collect::<Vec<_>>(), [PAD + W + GAP; 3]);
        assert_eq!(size, (PAD * 2. + 2. * W + GAP, PAD * 2. + 3. * (H + VGAP) - VGAP));
    }

    #[test]
    fn a_nested_agent_sits_a_column_right_of_its_parent_which_centres_on_it() {
        let (at, size) = layout(&[None, Some(0), None]);
        assert_eq!(at[2], (PAD + 2. * (W + GAP), PAD), "its own column, its parent's row");
        assert_eq!(at[1].1, at[2].1);
        assert_eq!(at[3].1, PAD + H + VGAP);
        assert_eq!(size.0, PAD * 2. + 3. * W + 2. * GAP);
    }

    #[test]
    fn an_edge_on_one_row_is_a_straight_line_and_one_across_rows_bends_in_the_gap() {
        assert_eq!(elbow((0., 10.), (44., 10.)), [(0., 10.), (44., 10.)]);
        let bent = elbow((0., 0.), (44., 40.));
        assert_eq!((bent[0], *bent.last().unwrap()), ((0., 0.), (44., 40.)));
        assert!(bent.contains(&(GAP / 2., 8.)) && bent.contains(&(GAP / 2., 32.)), "runs down the middle of the gap: {bent:?}");
        assert!(bent.iter().all(|p| (0. ..=44.).contains(&p.0) && (0. ..=40.).contains(&p.1)));
    }

    #[test]
    fn dashes_flow_along_the_edge_as_the_phase_moves() {
        let line = [(0., 0.), (20., 0.)];
        let starts = |phase| dashes(&line, 3., 5., phase).iter().map(|d| (d[0].0, d.last().unwrap().0)).collect::<Vec<_>>();
        assert_eq!(starts(0.), [(0., 3.), (8., 11.), (16., 19.)]);
        assert_eq!(starts(2.), [(2., 5.), (10., 13.), (18., 20.)]);
    }
}
