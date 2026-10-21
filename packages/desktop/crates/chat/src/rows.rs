use crate::{Block, End, ToolCall, ToolKind, ToolStatus, Transcript, Turn, Usage};
use std::ops::Range;

/// One list row; ids are stable across refreshes so the list keeps measured heights.
#[derive(Clone, Debug, PartialEq)]
pub enum Row {
    Prompt { turn: usize },
    /// The line heading a reply: who worked and for how long. It opens the reply's phases.
    Fold { turn: usize },
    /// A run of work: thinking and tool calls.
    Phase { turn: usize, blocks: Range<usize> },
    /// Sub-agents and commands that finished in the background, heading the turn the agent began itself to report on them.
    Notice { turn: usize },
    /// A `Block::Notice`: why the agent went on, heading the turn it set going, or an error it went on after, in place.
    Note { turn: usize, block: usize },
    /// The turn's latest todo list, as a card after its work.
    Tasks { turn: usize, block: usize },
    /// One markdown block (`range` into the message text) of a Text or Plan block.
    Text { turn: usize, block: usize, range: Range<usize> },
    /// An AskUserQuestion call: a marker that ends the phase before it.
    Question { turn: usize, block: usize },
    Compacted { turn: usize, block: usize },
    End { turn: usize },
    /// What the agent is doing, under the reply it's at work on (`turn` holds its fold). Rebuilt with the trailer.
    Live { turn: usize },
    /// "Sending…", while a send is on its way.
    Working,
    /// An outgoing message not yet in the transcript.
    Pending(usize),
}

impl Row {
    /// `{turn}`, `{turn}#notice`, `{turn}#fold`, `{turn}#p{first}`, `{turn}#{block}.{start}`, `{turn}#{block}`, `{turn}#end`, `{turn}#live`, `working`, `pending-{i}`.
    pub fn id(&self, t: &Transcript) -> String {
        let turn = |ix: usize| &t.turns[ix].id;
        match self {
            Row::Prompt { turn: ix } => turn(*ix).clone(),
            Row::Notice { turn: ix } => format!("{}#notice", turn(*ix)),
            Row::Fold { turn: ix } => format!("{}#fold", turn(*ix)),
            Row::Phase { turn: ix, blocks } => format!("{}#p{}", turn(*ix), blocks.start),
            Row::Text { turn: ix, block, range } => format!("{}#{block}.{}", turn(*ix), range.start),
            Row::Tasks { turn: ix, block } | Row::Question { turn: ix, block } | Row::Compacted { turn: ix, block } | Row::Note { turn: ix, block } => {
                format!("{}#{block}", turn(*ix))
            }
            Row::End { turn: ix } => format!("{}#end", turn(*ix)),
            Row::Live { turn: ix } => format!("{}#live", turn(*ix)),
            Row::Working => "working".into(),
            Row::Pending(i) => format!("pending-{i}"),
        }
    }

    pub fn turn(&self) -> Option<usize> {
        match self {
            Row::Prompt { turn }
            | Row::Notice { turn }
            | Row::Note { turn, .. }
            | Row::Fold { turn }
            | Row::Phase { turn, .. }
            | Row::Tasks { turn, .. }
            | Row::Text { turn, .. }
            | Row::Question { turn, .. }
            | Row::Compacted { turn, .. }
            | Row::End { turn } => Some(*turn),
            Row::Live { .. } | Row::Working | Row::Pending(_) => None,
        }
    }
}

/// The transcript's rows, then a row per pending send, "Sending…" while one is on its way, and what the agent is doing while at work.
pub fn rows(t: &Transcript, pending: usize, sending: bool) -> Vec<Row> {
    let mut rows = Vec::new();
    rows_from(&mut rows, t, 0, pending, sending);
    rows
}

/// Appends the rows of `t`'s turns from `from` on, then the trailer `rows` describes.
/// A turn lays out as its prompt or what it reports, the notices that set it going, the fold if it starts a reply, its
/// phases and the text between them, its todos, then its answer.
pub(crate) fn rows_from(rows: &mut Vec<Row>, t: &Transcript, from: usize, pending: usize, sending: bool) {
    for (ix, turn) in t.turns.iter().enumerate().skip(from) {
        if turn.prompt.is_some() {
            rows.push(Row::Prompt { turn: ix });
        }
        if !reported(t, ix).is_empty() {
            rows.push(Row::Notice { turn: ix });
        }
        let lead = turn.blocks.iter().take_while(|b| matches!(b, Block::Notice { .. })).count();
        rows.extend((0..lead).map(|block| Row::Note { turn: ix, block }));
        // A turn stopped or failed before anything came has nothing to fold.
        if folds_at(t, ix) && !(matches!(turn.end, Some(End::Failed { .. })) && turn.blocks.is_empty()) {
            rows.push(Row::Fold { turn: ix });
        }
        // Text after the turn's last step is the answer.
        let work = work_end(turn);
        let mut phase: Option<usize> = None;
        for (b, block) in turn.blocks[..work].iter().enumerate().skip(lead) {
            match block {
                _ if is_work(block) => _ = phase.get_or_insert(b),
                Block::Todos { .. } => {}
                _ => {
                    flush(rows, ix, &mut phase, b);
                    plain(rows, ix, b, block);
                }
            }
        }
        flush(rows, ix, &mut phase, work);
        if let Some(b) = turn.blocks.iter().rposition(|b| matches!(b, Block::Todos { .. })) {
            rows.push(Row::Tasks { turn: ix, block: b });
        }
        for (b, block) in turn.blocks.iter().enumerate().skip(work.max(lead)) {
            plain(rows, ix, b, block);
        }
        if turn.end.is_some() {
            rows.push(Row::End { turn: ix });
        }
    }
    rows.extend((0..pending).map(Row::Pending));
    if sending {
        rows.push(Row::Working);
    }
    if let Some(fold) = live_tail(t) {
        rows.push(Row::Live { turn: fold });
    }
}

/// A block's own rows outside any phase; todos have their place after the work.
fn plain(rows: &mut Vec<Row>, turn: usize, b: usize, block: &Block) {
    match block {
        Block::Text(m) | Block::Plan(m) => rows.extend(markdown_blocks(&m.text).into_iter().map(|range| Row::Text { turn, block: b, range })),
        Block::Tool(c) if matches!(c.kind, ToolKind::Question(_)) => rows.push(Row::Question { turn, block: b }),
        Block::Compacted { .. } => rows.push(Row::Compacted { turn, block: b }),
        Block::Notice { .. } => rows.push(Row::Note { turn, block: b }),
        _ => {}
    }
}

fn flush(rows: &mut Vec<Row>, turn: usize, phase: &mut Option<usize>, end: usize) {
    if let Some(start) = phase.take() {
        rows.push(Row::Phase { turn, blocks: start..end });
    }
}

/// A step of a phase: thinking, a tool call or a sub-agent, but not a question, which stands apart.
pub fn is_work(b: &Block) -> bool {
    match b {
        Block::Thinking(_) => true,
        Block::Tool(c) => !matches!(c.kind, ToolKind::Question(_)),
        _ => false,
    }
}

fn work_end(turn: &Turn) -> usize {
    turn.blocks.iter().rposition(is_work).map_or(0, |b| b + 1)
}

/// Whether block `block` of `turn` is text the agent wrote between its steps rather than its answer.
pub fn narrates(turn: &Turn, block: usize) -> bool {
    block < work_end(turn) && matches!(turn.blocks.get(block), Some(Block::Text(_)))
}

fn compaction(turn: &Turn) -> bool {
    turn.prompt.is_none() && !turn.blocks.is_empty() && turn.blocks.iter().all(|b| matches!(b, Block::Compacted { .. }))
}

/// Whether turn `ix` heads a reply with a fold: a prompt's turn, unless it steered the one before, or a turn the agent
/// begins itself, as when a hook or a background task sets it going, other than a compaction.
fn folds_at(t: &Transcript, ix: usize) -> bool {
    let turn = &t.turns[ix];
    if turn.prompt.is_some() { !steered(t, ix) } else { !compaction(turn) }
}

/// Whether turn `ix`'s prompt came while the agent was still on the turn before, which it carries on: that turn never
/// ends, as the agent ends and times the two as one.
fn steered(t: &Transcript, ix: usize) -> bool {
    t.turns[ix].prompt.is_some() && ix.checked_sub(1).is_some_and(|prev| working(&t.turns[prev]))
}

/// The turn whose fold heads the reply `turn` belongs to.
pub fn fold_of(t: &Transcript, turn: usize) -> Option<usize> {
    (0..=turn.min(t.turns.len().checked_sub(1)?)).rev().find(|&ix| folds_at(t, ix))
}

/// The turns of the reply whose fold sits on `turn`.
pub fn folded(t: &Transcript, turn: usize) -> Range<usize> {
    let end = (turn + 1..t.turns.len()).find(|&ix| folds_at(t, ix) || compaction(&t.turns[ix])).unwrap_or(t.turns.len());
    turn..end
}

/// Whether the agent is still at work on `turns`: the last of them is open, or a sub-agent one started runs on.
pub fn at_work_on(t: &Transcript, turns: Range<usize>) -> bool {
    let open = turns.end == t.turns.len() && turns.end > turns.start && working(&t.turns[turns.end - 1]);
    open || t.turns[turns].iter().flat_map(|turn| &turn.blocks).any(running_agent)
}

/// The fold of the last reply while the agent is at work on it, or on a sub-agent of any reply in the background; the
/// live line sits under the newest exchange rather than on a fold.
pub fn live_tail(t: &Transcript) -> Option<usize> {
    let fold = fold_of(t, t.turns.len().checked_sub(1)?)?;
    at_work_on(t, 0..t.turns.len()).then_some(fold)
}

/// The turn whose fold held the live line before, `before`, once `t`'s no longer does: its reply just settled.
pub fn settled_tail(before: Option<&str>, t: &Transcript) -> Option<String> {
    let before = before?;
    (live_tail(t).map(|f| t.turns[f].id.as_str()) != Some(before)).then(|| before.to_string())
}

/// Sub-agents and commands still running in `turns` after their own turn ended, the ones working in the background, with
/// their turn and block.
pub fn in_background(turns: &[Turn]) -> impl Iterator<Item = (usize, usize, &ToolCall)> {
    let ended = turns.iter().enumerate().filter(|(_, turn)| turn.end.is_some());
    ended.flat_map(|(t, turn)| {
        turn.blocks.iter().enumerate().filter_map(move |(b, block)| match block {
            Block::Tool(c) if c.status == ToolStatus::Running && matches!(c.kind, ToolKind::Agent(_) | ToolKind::Shell { .. }) => Some((t, b, c)),
            _ => None,
        })
    })
}

/// The prompt row a step up or down lands on from the top row `top`: up goes to the start of the exchange showing, or
/// the one before if already there.
pub fn step_prompt(rows: &[Row], top: usize, down: bool) -> Option<usize> {
    let mut prompts = rows.iter().enumerate().filter(|(_, r)| matches!(r, Row::Prompt { .. })).map(|(ix, _)| ix);
    if down { prompts.find(|&ix| ix > top) } else { prompts.rev().find(|&ix| ix < top) }
}

/// The row that shows block `block` of turn `turn`: the phase holding it.
pub fn row_of(rows: &[Row], turn: usize, block: usize) -> Option<usize> {
    rows.iter().position(|row| matches!(row, Row::Phase { turn: t, blocks } if *t == turn && blocks.contains(&block)))
}

/// The sub-agents and commands turn `ix` reports on: it's one the agent began itself, and they finished after their own
/// turn ended, since the turn before `ix` began and by the time `ix` did.
pub fn reported(t: &Transcript, ix: usize) -> Vec<&ToolCall> {
    let turn = &t.turns[ix];
    let (Some(at), Some(since)) = (start(turn), ix.checked_sub(1).and_then(|p| start(&t.turns[p]))) else { return Vec::new() };
    if turn.prompt.is_some() || compaction(turn) {
        return Vec::new();
    }
    let ended = t.turns[..ix].iter().filter_map(|own| Some((own, ended_at(own)?)));
    ended
        .flat_map(|(own, end)| {
            own.blocks.iter().filter_map(move |b| match b {
                Block::Tool(c) if matches!(c.kind, ToolKind::Agent(_) | ToolKind::Shell { .. }) && c.status != ToolStatus::Running => {
                    let done = c.at + c.duration_ms? as i64;
                    (done > end && done > since && done <= at).then_some(c)
                }
                _ => None,
            })
        })
        .collect()
}

/// When the agent began `turn`: its prompt, else its first timed block.
fn start(turn: &Turn) -> Option<i64> {
    let block_at = |b: &Block| match b {
        Block::Text(m) | Block::Thinking(m) | Block::Plan(m) | Block::Notice { message: m, .. } => Some(m.at),
        Block::Tool(c) => Some(c.at),
        _ => None,
    };
    turn.prompt.as_ref().map(|p| p.at).or_else(|| turn.blocks.iter().find_map(block_at))
}

fn ended_at(turn: &Turn) -> Option<i64> {
    match turn.end {
        Some(End::Done { duration_ms, .. } | End::Failed { duration_ms, .. }) => Some(start(turn)? + duration_ms as i64),
        _ => None,
    }
}

/// Whether `b` is a sub-agent still at work.
pub fn running_agent(b: &Block) -> bool {
    matches!(b, Block::Tool(c) if c.status == ToolStatus::Running && matches!(c.kind, ToolKind::Agent(_)))
}

/// The turns of the reply `turn` closes, or None while a later turn carries it on. A reply is a turn and the steers
/// sent into it.
pub fn reply(t: &Transcript, turn: usize) -> Option<Range<usize>> {
    if turn + 1 < t.turns.len() && steered(t, turn + 1) {
        return None;
    }
    let start = (0..=turn).rev().find(|&ix| ix == 0 || !steered(t, ix))?;
    Some(start..turn + 1)
}

/// What a reply that ended well took, summed over its turns; None when its last turn failed or is still open.
pub fn worked(t: &Transcript, turns: Range<usize>) -> Option<(u64, Option<Usage>)> {
    let turns = &t.turns[turns];
    if !matches!(turns.last()?.end, Some(End::Done { .. })) {
        return None;
    }
    let done = turns.iter().filter_map(|turn| if let Some(End::Done { duration_ms, usage }) = &turn.end { Some((*duration_ms, *usage)) } else { None });
    Some(done.fold((0, None), |(ms, sum), (d, u)| {
        let sum = match (sum, u) {
            (Some(a), Some(b)) => Some(Usage { input: a.input + b.input, output: a.output + b.output, cache_read: a.cache_read + b.cache_read }),
            (a, b) => a.or(b),
        };
        (ms + d, sum)
    }))
}

/// Whether the agent is on a turn.
pub fn at_work(t: &Transcript) -> bool {
    t.turns.last().is_some_and(working)
}

/// An open turn holding only a compaction is the agent tidying up between prompts, not working.
pub(crate) fn working(turn: &Turn) -> bool {
    turn.end.is_none() && (turn.prompt.is_some() || !turn.blocks.iter().all(|b| matches!(b, Block::Compacted { .. })))
}

/// 16 over the first row and 14 at each exchange; within one, 6 between parts, 10 between markdown blocks of one
/// message, and none above a phase, whose rail runs on from the fold, or "Sending…", which brings its own.
pub fn top_gap(prev: Option<&Row>, row: &Row) -> f32 {
    let Some(prev) = prev else { return 16. };
    match (prev, row) {
        (_, Row::Prompt { .. } | Row::Pending(_)) | (Row::Pending(_), Row::Live { .. }) => 14.,
        (_, Row::Phase { .. } | Row::Working) => 0.,
        (Row::Text { turn: a, block: x, .. }, Row::Text { turn: b, block: y, .. }) if (a, x) == (b, y) => 10.,
        (_, Row::Tasks { .. }) => 12.,
        (_, Row::Question { .. } | Row::Compacted { .. } | Row::Notice { .. } | Row::Note { .. }) => 8.,
        _ => 6.,
    }
}

/// A phase's headline: what the work did, as "Edited a.rs · Ran 2 commands", in the present tense while `live`;
/// sub-agents go by their own state, as "Using a subagent".
pub fn phase_title(blocks: &[Block], live: bool) -> String {
    let calls = || blocks.iter().filter_map(|b| if let Block::Tool(c) = b { Some(c) } else { None });
    let mut edited: Vec<&str> = Vec::new();
    for c in calls() {
        if let ToolKind::Edit { path, .. } | ToolKind::Write { path, .. } = &c.kind
            && !edited.contains(&path.as_str())
        {
            edited.push(path);
        }
    }
    let explored = calls().any(|c| matches!(c.kind, ToolKind::Read { .. } | ToolKind::Search { .. } | ToolKind::Fetch { .. }));
    let ran = calls().filter(|c| matches!(c.kind, ToolKind::Shell { .. })).count();
    let mut parts = Vec::new();
    let agents = calls().filter(|c| matches!(c.kind, ToolKind::Agent(_)));
    let using = if agents.clone().any(|c| c.status == ToolStatus::Running) { "Using" } else { "Used" };
    match agents.count() {
        0 => {}
        1 => parts.push(format!("{using} a subagent")),
        n => parts.push(format!("{using} {n} subagents")),
    }
    match edited[..] {
        [] => {}
        [path] => parts.push(format!("{} {}", if live { "Editing" } else { "Edited" }, path.rsplit('/').next().unwrap_or(path))),
        _ => parts.push(format!("{} {} files", if live { "Editing" } else { "Edited" }, edited.len())),
    }
    if explored {
        parts.push(if live { "Exploring the project" } else { "Explored the project" }.to_string());
    }
    match ran {
        0 => {}
        1 => parts.push(if live { "Executing a command" } else { "Ran a command" }.to_string()),
        n => parts.push(format!("{} {n} commands", if live { "Executing" } else { "Ran" })),
    }
    if parts.is_empty() && blocks.iter().all(|b| matches!(b, Block::Thinking(_))) {
        return if live { "Thinking" } else { "Thought" }.to_string();
    }
    if parts.is_empty() { "Working".to_string() } else { parts.join(" · ") }
}

/// The range of `old` to replace and how many new rows replace it, sharing the longest equal prefix.
pub fn splice(old: &[String], new: &[String]) -> (Range<usize>, usize) {
    let shared = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    (shared..old.len(), new.len() - shared)
}

/// Byte ranges of top-level markdown blocks, never splitting a fenced code block.
/// Blank lines end a block unless a list carries on past them.
pub fn markdown_blocks(text: &str) -> Vec<Range<usize>> {
    let mut blocks = Vec::new();
    let mut current: Option<Range<usize>> = None;
    let mut fence: Option<(u8, usize)> = None;
    let (mut list, mut after_blank) = (false, false);
    let mut start = 0;
    for line in text.split_inclusive('\n') {
        let range = start..start + line.trim_end_matches(['\n', '\r']).len();
        start += line.len();
        let content = &text[range.clone()];
        if let Some((ch, len)) = fence {
            current.as_mut().expect("a fence is open").end = range.end;
            if closes_fence(content, ch, len) {
                fence = None;
                blocks.extend(current.take());
            }
            continue;
        }
        if content.trim().is_empty() {
            after_blank = true;
            continue;
        }
        let item = list_item(content);
        let indented = content.starts_with("  ") || content.starts_with('\t');
        fence = opens_fence(content);
        let heading = heading(content);
        let continues = !after_blank || (list && (item || indented));
        match &mut current {
            Some(block) if continues && fence.is_none() && !heading => {
                block.end = range.end;
                list |= item;
            }
            _ => {
                blocks.extend(current.replace(range));
                list = item;
            }
        }
        if heading {
            blocks.extend(current.take());
        }
        after_blank = false;
    }
    blocks.extend(current);
    blocks
}

fn indent(line: &str) -> Option<&str> {
    let rest = line.trim_start_matches(' ');
    (line.len() - rest.len() <= 3).then_some(rest)
}

fn opens_fence(line: &str) -> Option<(u8, usize)> {
    let rest = indent(line)?;
    let ch = *rest.as_bytes().first().filter(|c| matches!(c, b'`' | b'~'))?;
    let len = rest.bytes().take_while(|&c| c == ch).count();
    (len >= 3 && !(ch == b'`' && rest[len..].contains('`'))).then_some((ch, len))
}

fn closes_fence(line: &str, ch: u8, len: usize) -> bool {
    indent(line).is_some_and(|rest| {
        let run = rest.bytes().take_while(|&c| c == ch).count();
        run >= len && rest[run..].trim().is_empty()
    })
}

fn heading(line: &str) -> bool {
    indent(line).is_some_and(|rest| {
        let hashes = rest.bytes().take_while(|&c| c == b'#').count();
        (1..=6).contains(&hashes) && rest[hashes..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
    })
}

fn list_item(line: &str) -> bool {
    let rest = line.trim_start();
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let marker = match digits {
        0 => rest.starts_with(['-', '*', '+']) as usize,
        1..=9 if rest[digits..].starts_with(['.', ')']) => digits + 1,
        _ => 0,
    };
    marker > 0 && rest[marker..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{End, Message, SubAgent};

    fn msg(text: &str) -> Message {
        Message { id: "m".into(), at: 0, text: text.into() }
    }

    fn call(kind: ToolKind, status: ToolStatus) -> Block {
        Block::Tool(ToolCall { id: "t".into(), name: String::new(), kind, status, output: None, duration_ms: None, at: 0 })
    }

    fn shell() -> Block {
        call(ToolKind::Shell { command: "ls".into() }, ToolStatus::Ok)
    }

    fn agent() -> Block {
        call(ToolKind::Agent(SubAgent::default()), ToolStatus::Running)
    }

    fn think() -> Block {
        Block::Thinking(msg("hmm"))
    }

    fn text(t: &str) -> Block {
        Block::Text(msg(t))
    }

    fn done() -> Option<End> {
        Some(End::Done { duration_ms: 1, usage: None })
    }

    fn turn(id: &str, blocks: Vec<Block>, end: Option<End>) -> Turn {
        Turn { id: id.into(), prompt: Some(msg("go")), blocks, end }
    }

    fn one(blocks: Vec<Block>) -> Transcript {
        Transcript { turns: vec![turn("u1", blocks, done())] }
    }

    fn ids(t: &Transcript, pending: usize) -> Vec<String> {
        rows(t, pending, pending > 0).iter().map(|r| r.id(t)).collect()
    }

    fn ids_of(t: &Transcript) -> Vec<String> {
        rows(t, 0, false).iter().map(|r| r.id(t)).collect()
    }

    #[test]
    fn a_reply_lays_out_as_fold_phases_then_its_answer() {
        let t = one(vec![think(), shell(), shell(), text("Done.")]);
        assert_eq!(
            rows(&t, 0, false),
            vec![Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::Phase { turn: 0, blocks: 0..3 }, Row::Text { turn: 0, block: 3, range: 0..5 }, Row::End { turn: 0 }]
        );
    }

    #[test]
    fn text_between_steps_stands_whole_between_their_phases() {
        let t = one(vec![shell(), text("Now the tests.\n\nThey cover both paths."), think(), shell(), text("All green.")]);
        assert_eq!(ids_of(&t), ["u1", "u1#fold", "u1#p0", "u1#1.0", "u1#1.16", "u1#p2", "u1#4.0", "u1#end"]);
        assert_eq!(rows(&t, 0, false)[5], Row::Phase { turn: 0, blocks: 2..4 });
        assert!(narrates(&t.turns[0], 1) && !narrates(&t.turns[0], 4), "the answer after the last step isn't between steps");
    }

    #[test]
    fn sub_agents_are_steps_of_the_phase_they_run_in_and_the_latest_todos_follow_the_work() {
        let t = one(vec![todos("a"), shell(), agent(), shell(), agent(), todos("b"), text("Both reported.")]);
        assert_eq!(ids_of(&t), ["u1", "u1#fold", "u1#p1", "u1#5", "u1#6.0", "u1#end", "u1#live"], "the running sub-agents keep the reply at work");
        assert_eq!(rows(&t, 0, false)[2], Row::Phase { turn: 0, blocks: 1..5 });
        assert!(matches!(rows(&t, 0, false)[3], Row::Tasks { block: 5, .. }));
    }

    #[test]
    fn narration_before_a_sub_agent_stands_above_its_phase() {
        let t = one(vec![text("I'll hand the audit to a subagent."), agent(), text("It's on it.")]);
        assert_eq!(
            rows(&t, 0, false)[2..5],
            [Row::Text { turn: 0, block: 0, range: 0..34 }, Row::Phase { turn: 0, blocks: 1..2 }, Row::Text { turn: 0, block: 2, range: 0..11 }]
        );
    }

    #[test]
    fn a_reply_stopped_before_anything_came_has_no_fold() {
        let stopped = Some(End::Failed { error: "interrupted".into(), duration_ms: 0 });
        assert_eq!(ids_of(&Transcript { turns: vec![turn("u1", vec![], stopped)] }), ["u1", "u1#end"]);
        assert_eq!(ids_of(&Transcript { turns: vec![turn("u1", vec![], None)] }), ["u1", "u1#fold", "u1#live"], "an open one keeps it for its live line");
    }

    #[test]
    fn a_question_ends_the_phase_and_stands_alone() {
        let q = call(ToolKind::Question(vec![]), ToolStatus::Ok);
        let t = one(vec![shell(), q, shell()]);
        assert_eq!(rows(&t, 0, false)[2..5], [Row::Phase { turn: 0, blocks: 0..1 }, Row::Question { turn: 0, block: 1 }, Row::Phase { turn: 0, blocks: 2..3 }]);
    }

    fn todos(text: &str) -> Block {
        Block::Todos { id: text.into(), todos: vec![crate::Todo { text: text.into(), status: crate::TodoStatus::Pending }] }
    }

    #[test]
    fn a_turn_the_agent_begins_itself_heads_its_own_reply() {
        let own = Turn { prompt: None, ..turn("n1", vec![shell(), text("reported")], done()) };
        let compact = Turn { prompt: None, ..turn("c", vec![Block::Compacted { id: "c".into(), auto: true }], done()) };
        let after = Turn { prompt: None, ..turn("n2", vec![text("resumed")], None) };
        let t = Transcript { turns: vec![turn("u1", vec![agent()], done()), own, compact, after] };
        let folds: Vec<_> = rows(&t, 0, false).iter().filter_map(|r| if let Row::Fold { turn } = r { Some(*turn) } else { None }).collect();
        assert_eq!(folds, [0, 1, 3], "a compaction gets none");
        assert_eq!((fold_of(&t, 1), folded(&t, 0)), (Some(1), 0..1));
        assert_eq!((fold_of(&t, 3), folded(&t, 3)), (Some(3), 3..4));
    }

    #[test]
    fn a_reply_is_at_work_while_its_turn_is_open_or_a_sub_agent_it_started_runs() {
        let t = Transcript { turns: vec![turn("u1", vec![agent(), agent()], done()), turn("u2", vec![shell()], None)] };
        assert!(at_work_on(&t, 0..1) && at_work_on(&t, 1..2));
        assert_eq!(in_background(&t.turns[0..1]).count(), 2);
        assert_eq!(in_background(&t.turns[1..2]).count(), 0, "an open turn's agents aren't in the background yet");
        assert!(!at_work_on(&one(vec![shell()]), 0..1));
    }

    #[test]
    fn a_phase_is_titled_by_its_work() {
        let edit = |p: &str| call(ToolKind::Edit { path: p.into(), diff: None }, ToolStatus::Ok);
        let read = call(ToolKind::Read { path: "a".into() }, ToolStatus::Ok);
        assert_eq!(phase_title(&[edit("src/a.rs"), edit("src/a.rs"), shell()], false), "Edited a.rs · Ran a command");
        assert_eq!(phase_title(&[read.clone(), edit("a"), edit("b"), shell(), shell()], true), "Editing 2 files · Exploring the project · Executing 2 commands");
        assert_eq!(phase_title(&[think()], false), "Thought");
        assert_eq!(phase_title(&[call(ToolKind::Other { input: String::new(), description: String::new() }, ToolStatus::Ok)], false), "Working");
        let sub = |status| call(ToolKind::Agent(SubAgent::default()), status);
        assert_eq!(phase_title(&[sub(ToolStatus::Running)], false), "Using a subagent", "it runs on in the background");
        assert_eq!(phase_title(&[sub(ToolStatus::Ok), shell()], true), "Used a subagent · Executing a command");
        assert_eq!(phase_title(&[sub(ToolStatus::Ok), sub(ToolStatus::Failed)], false), "Used 2 subagents");
    }

    #[test]
    fn a_long_message_becomes_one_row_per_markdown_block() {
        let md = "## Step 1\nIntro line\nmore intro\n\n- one\n- two\n\n- three\n  continued\n\nLast.";
        let parts: Vec<_> = markdown_blocks(md).into_iter().map(|r| &md[r]).collect();
        assert_eq!(parts, vec!["## Step 1", "Intro line\nmore intro", "- one\n- two\n\n- three\n  continued", "Last."]);
        let t = one(vec![text(md)]);
        assert_eq!(rows(&t, 0, false).iter().filter(|r| matches!(r, Row::Text { .. })).count(), 4);
    }

    #[test]
    fn a_fenced_code_block_is_never_split() {
        let md = "Run:\n```sh\nmake\n\n## not a heading\n```\nAfter\n\n~~~~\n```\n\n~~~~\n\n```unclosed\n\nrest";
        let parts: Vec<_> = markdown_blocks(md).into_iter().map(|r| &md[r]).collect();
        assert_eq!(parts, vec!["Run:", "```sh\nmake\n\n## not a heading\n```", "After", "~~~~\n```\n\n~~~~", "```unclosed\n\nrest"]);
    }

    #[test]
    fn inline_backticks_open_no_fence() {
        let md = "```inline``` code\n\nnext";
        assert_eq!(markdown_blocks(md).len(), 2);
    }

    #[test]
    fn row_ids_stay_the_same_when_a_tool_finishes() {
        let running = Transcript { turns: vec![turn("u1", vec![call(ToolKind::Shell { command: "ls".into() }, ToolStatus::Running)], None)] };
        let finished = Transcript { turns: vec![turn("u1", vec![shell(), text("done")], None)] };
        let (before, after) = (ids(&running, 0), ids(&finished, 0));
        assert_eq!(before, vec!["u1", "u1#fold", "u1#p0", "u1#live"]);
        assert_eq!(after, vec!["u1", "u1#fold", "u1#p0", "u1#1.0", "u1#live"]);
    }

    #[test]
    fn exchanges_get_fourteen_and_their_parts_six() {
        let t = Transcript {
            turns: vec![turn("u1", vec![shell(), text("a\n\nb"), Block::Compacted { id: "c".into(), auto: true }], done()), turn("u2", vec![], None)],
        };
        let rows = rows(&t, 1, true);
        let gaps: Vec<_> = rows.iter().enumerate().map(|(i, r)| top_gap(i.checked_sub(1).map(|p| &rows[p]), r)).collect();
        // prompt, fold, phase, a, b, compacted, end, prompt, fold, pending, sending, live
        assert_eq!(gaps, vec![16., 6., 0., 6., 10., 8., 6., 14., 6., 14., 0., 6.]);
    }

    #[test]
    fn splice_replaces_only_rows_after_the_shared_prefix() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(splice(&s(&["a", "b", "working"]), &s(&["a", "b", "c", "working"])), (2..3, 2));
        assert_eq!(splice(&s(&["a", "b"]), &s(&["a", "b"])), (2..2, 0));
        assert_eq!(splice(&s(&["a", "b"]), &s(&["a"])), (1..2, 0));
    }

    #[test]
    fn sending_shows_only_while_a_send_is_on_its_way() {
        let open = Transcript { turns: vec![turn("u1", vec![], done()), turn("u2", vec![shell()], None)] };
        assert_eq!(rows(&open, 0, false).last(), Some(&Row::Live { turn: 1 }), "the live line says the agent works");
        let ended = one(vec![shell()]);
        assert_eq!(rows(&ended, 0, false).last(), Some(&Row::End { turn: 0 }));
        assert_eq!(rows(&ended, 1, true)[4..], [Row::Pending(0), Row::Working]);
        assert_eq!(rows(&ended, 1, false)[4..], [Row::Pending(0)], "a send not delivered waits on no one");
        let compacted = Transcript { turns: vec![Turn { id: "c".into(), prompt: None, blocks: vec![Block::Compacted { id: "c".into(), auto: true }], end: None }] };
        assert_eq!(rows(&compacted, 0, false), vec![Row::Compacted { turn: 0, block: 0 }]);
    }

    #[test]
    fn the_live_line_trails_only_the_last_reply_while_the_agent_works_on_it() {
        let own = Turn { prompt: None, ..turn("n1", vec![shell()], None) };
        let t = Transcript { turns: vec![turn("u1", vec![agent()], done()), turn("u2", vec![shell()], done()), own] };
        assert_eq!(live_tail(&t), Some(2), "the turn the agent began itself heads its own reply");
        assert!(rows(&t, 1, true).ends_with(&[Row::Phase { turn: 2, blocks: 0..1 }, Row::Pending(0), Row::Working, Row::Live { turn: 2 }]), "a send waits above the live line");
        assert_eq!(live_tail(&one(vec![shell()])), None);
        assert_eq!(live_tail(&Transcript::default()), None);
    }

    #[test]
    fn a_live_tail_settles_once_its_fold_stops_carrying_the_live_line() {
        let own = Turn { prompt: None, ..turn("n1", vec![shell()], None) };
        let working = Transcript { turns: vec![turn("u1", vec![agent()], done()), turn("u2", vec![shell()], done()), own.clone()] };
        let ended = Transcript { turns: vec![turn("u1", vec![agent()], done()), turn("u2", vec![shell()], done()), Turn { end: done(), ..own }] };
        assert_eq!(settled_tail(Some("n1"), &working), None);
        assert_eq!(settled_tail(Some("n1"), &ended), None, "u1's sub-agent still works in the background");
        let mut ended = ended;
        ended.turns[0].blocks = vec![call(ToolKind::Agent(SubAgent::default()), ToolStatus::Ok)];
        assert_eq!(settled_tail(Some("n1"), &ended).as_deref(), Some("n1"));
        assert_eq!(settled_tail(None, &ended), None);
    }

    #[test]
    fn a_command_left_in_the_background_counts_but_leaves_the_agent_idle() {
        let server = call(ToolKind::Shell { command: "bun run dev".into() }, ToolStatus::Running);
        let t = Transcript { turns: vec![turn("u1", vec![server, shell()], done())] };
        assert_eq!(in_background(&t.turns).count(), 1);
        assert!(!at_work_on(&t, 0..1));
    }

    #[test]
    fn a_background_task_is_found_at_its_turn_and_block_and_shown_by_its_row() {
        let server = call(ToolKind::Shell { command: "bun run dev".into() }, ToolStatus::Running);
        let t = Transcript { turns: vec![turn("u1", vec![shell()], done()), turn("u2", vec![shell(), server, agent()], done())] };
        let found: Vec<_> = in_background(&t.turns).map(|(turn, block, _)| (turn, block)).collect();
        assert_eq!(found, [(1, 1), (1, 2)]);
        let rows = rows(&t, 0, false);
        let shown = |turn, block| row_of(&rows, turn, block).map(|ix| rows[ix].clone());
        assert_eq!(shown(1, 1), Some(Row::Phase { turn: 1, blocks: 0..3 }), "a command shows in the phase that holds it");
        assert_eq!(shown(1, 2), Some(Row::Phase { turn: 1, blocks: 0..3 }), "and so does a sub-agent");
        assert_eq!(shown(1, 5), None);
    }

    #[test]
    fn a_background_sub_agent_keeps_the_agent_at_work_after_its_turn_ends() {
        let agent = |status| call(ToolKind::Agent(SubAgent::default()), status);
        let running = Transcript { turns: vec![turn("u1", vec![agent(ToolStatus::Running)], done()), turn("u2", vec![shell()], done())] };
        assert!(at_work_on(&running, 0..1), "its turn ended, but its sub-agent runs on");
        let finished = Transcript { turns: vec![turn("u1", vec![agent(ToolStatus::Ok)], done())] };
        assert!(!at_work_on(&finished, 0..1));
    }

    #[test]
    fn the_live_line_follows_the_newest_exchange_while_a_sub_agent_runs_in_the_background() {
        let t = Transcript { turns: vec![turn("u1", vec![agent()], done()), turn("u2", vec![shell()], done())] };
        assert_eq!(live_tail(&t), Some(1));
        assert_eq!(rows(&t, 0, false).last(), Some(&Row::Live { turn: 1 }));
    }

    #[test]
    fn a_turn_the_agent_begins_after_a_background_sub_agent_finished_reports_it() {
        let sub = |at, ms| Block::Tool(ToolCall { duration_ms: Some(ms), at, ..ToolCall { id: "a".into(), name: String::new(), kind: ToolKind::Agent(SubAgent::default()), status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 } });
        let own = Turn { id: "n1".into(), prompt: None, blocks: vec![Block::Text(Message { id: "r".into(), at: 10_500, text: "It found two.".into() })], end: done() };
        let ended = |blocks| Turn { end: Some(End::Done { duration_ms: 1_000, usage: None }), ..turn("u1", blocks, None) };
        let t = Transcript { turns: vec![ended(vec![sub(100, 10_000)]), own.clone()] };
        assert_eq!(reported(&t, 1).len(), 1);
        assert_eq!(ids_of(&t)[..2], ["u1", "u1#fold"]);
        assert!(ids_of(&t).contains(&"n1#notice".to_string()));
        let waited = Transcript { turns: vec![ended(vec![sub(100, 500)]), own] };
        assert!(reported(&waited, 1).is_empty(), "the turn waited for it, so it reported in that turn");
    }

    #[test]
    fn each_turn_the_agent_begins_itself_is_a_reply_timed_on_its_own() {
        let own = |id: &str, ms: u64, input: u64| Turn {
            prompt: None,
            ..turn(id, vec![text("a sub-agent reported")], Some(End::Done { duration_ms: ms, usage: Some(Usage { input, output: 1, cache_read: 0 }) }))
        };
        let compact = Turn { prompt: None, ..turn("c", vec![Block::Compacted { id: "c".into(), auto: true }], done()) };
        let t = Transcript { turns: vec![turn("u1", vec![agent()], done()), own("n1", 5_000, 10), own("n2", 7_000, 20), compact, turn("u2", vec![shell()], None)] };
        assert_eq!((reply(&t, 0), reply(&t, 1), reply(&t, 2)), (Some(0..1), Some(1..2), Some(2..3)), "a footer under each");
        assert_eq!(reply(&t, 3), Some(3..4));
        assert_eq!(worked(&t, 2..3), Some((7_000, Some(Usage { input: 20, output: 1, cache_read: 0 }))));
        assert_eq!(worked(&t, 4..5), None, "an open turn hasn't worked yet");
    }

    #[test]
    fn what_set_the_agent_going_heads_its_turn_and_an_error_it_went_on_after_stands_in_place() {
        let notice = |text: &str, error| Block::Notice { message: msg(text), error };
        let own = Turn { prompt: None, ..turn("n1", vec![notice("Stop hook: run the tests", false), shell(), notice("API Error", true), shell(), text("Done.")], done()) };
        let t = Transcript { turns: vec![turn("u1", vec![text("Hi.")], done()), own] };
        assert_eq!(ids_of(&t)[4..], ["n1#0", "n1#fold", "n1#p1", "n1#2", "n1#p3", "n1#4.0", "n1#end"]);
        assert_eq!(rows(&t, 0, false)[4], Row::Note { turn: 1, block: 0 });
    }

    #[test]
    fn a_command_that_finished_in_the_background_is_reported_like_a_sub_agent() {
        let cmd = Block::Tool(ToolCall { id: "b".into(), name: String::new(), kind: ToolKind::Shell { command: "cargo test".into() }, status: ToolStatus::Failed, output: None, duration_ms: Some(10_000), at: 100 });
        let own = Turn { prompt: None, ..turn("n1", vec![Block::Text(Message { id: "r".into(), at: 10_500, text: "It failed.".into() })], done()) };
        let t = Transcript { turns: vec![turn("u1", vec![cmd], Some(End::Done { duration_ms: 1_000, usage: None })), own] };
        assert_eq!(reported(&t, 1).len(), 1);
    }

    #[test]
    fn a_call_that_finished_after_its_turn_failed_is_reported_by_the_turn_it_set_going() {
        let cmd = |ms| Block::Tool(ToolCall { id: "b".into(), name: String::new(), kind: ToolKind::Shell { command: "cargo test".into() }, status: ToolStatus::Ok, output: None, duration_ms: Some(ms), at: 100 });
        let failed = |blocks| turn("u1", blocks, Some(End::Failed { error: "API Error: Connection lost".into(), duration_ms: 1_000 }));
        let own = Turn { prompt: None, ..turn("n1", vec![Block::Text(Message { id: "r".into(), at: 10_500, text: "Tests pass.".into() })], done()) };
        assert_eq!(reported(&Transcript { turns: vec![failed(vec![cmd(10_000)]), own.clone()] }, 1).len(), 1);
        assert!(reported(&Transcript { turns: vec![failed(vec![cmd(500)]), own] }, 1).is_empty(), "it ended before the error did");
    }

    #[test]
    fn a_prompt_sent_mid_turn_carries_on_the_reply_it_came_into() {
        let t = Transcript { turns: vec![turn("u1", vec![think(), shell()], None), turn("u2", vec![shell(), text("Done.")], done())] };
        assert_eq!(ids_of(&t), ["u1", "u1#fold", "u1#p0", "u2", "u2#p0", "u2#1.0", "u2#end"], "the steer stands between the work it divides");
        assert_eq!((fold_of(&t, 1), folded(&t, 0)), (Some(0), 0..2));
        assert_eq!((reply(&t, 0), reply(&t, 1)), (None, Some(0..2)), "one footer for the whole reply");
        assert_eq!(worked(&t, 0..2), Some((1, None)), "the agent times the turn once");
    }

    #[test]
    fn a_reply_stays_at_work_while_the_agent_works_past_a_steer() {
        let t = Transcript { turns: vec![turn("u1", vec![think(), shell()], None), turn("u2", vec![shell()], None)] };
        assert!(at_work_on(&t, folded(&t, 0)), "the work before the steer stays open");
        assert_eq!(live_tail(&t), Some(0));
    }

    #[test]
    fn a_step_up_lands_on_the_showing_prompt_before_the_one_above_it() {
        let mut rows = vec![Row::Pending(0); 21];
        for ix in [2, 10, 20] {
            rows[ix] = Row::Prompt { turn: ix };
        }
        assert_eq!(step_prompt(&rows, 12, false), Some(10), "back to the start of the exchange showing");
        assert_eq!(step_prompt(&rows, 10, false), Some(2));
        assert_eq!(step_prompt(&rows, 2, false), None);
        assert_eq!(step_prompt(&rows, 12, true), Some(20));
        assert_eq!(step_prompt(&rows, 0, true), Some(2), "above every prompt, down goes to the first");
        assert_eq!(step_prompt(&rows, 20, true), None);
    }
}
