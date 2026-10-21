use crate::rows::rows_from;
use crate::{splice, Backend, Block, Delivery, Fold, Outgoing, Row, Transcript, Turn};
use std::ops::Range;

/// Open/closed choices the user made, by row id: folds, phases and sub-agents.
pub type Folds = std::collections::HashMap<String, bool>;

/// How the rows moved: those in the first range hold their ids but show something new, and the second range's rows
/// gave way to as many others as the count.
pub type Change = (Range<usize>, Range<usize>, usize);

/// One agent's chat as the view shows it; rebuilt only when the agent's items change.
#[derive(Default)]
pub struct Session {
    pub transcript: Transcript,
    pub rows: Vec<Row>,
    pub ids: Vec<String>,
    pub folds: Folds,
    pub outgoing: Outgoing,
    revision: u64,
    fold: Fold,
    /// Sends the transcript shows, and whether one is still on its way.
    sends: (usize, bool),
}

impl Session {
    /// Refolds when `revision` moved, settles outgoing sends by the transcript and pocketd's status (`working`), and
    /// returns how the rows moved, if they did.
    pub fn update<B: Backend + ?Sized>(&mut self, backend: &B, revision: u64, items: &[B::Item], working: bool, now_ms: i64) -> Option<Change> {
        let old = (revision != self.revision).then(|| {
            self.revision = revision;
            self.outgoing.moved();
            let turns = self.transcript.turns.len();
            let replaced = backend.refold(items, &mut self.transcript, &mut self.fold);
            (turns - replaced.len(), replaced)
        });
        self.outgoing.settle(&self.transcript, working, now_ms);
        let sends = (self.outgoing.echoed().count(), self.outgoing.echoed().any(|(_, p)| p.delivery == Delivery::Sending));
        if old.is_none() && sends == self.sends {
            return None;
        }
        self.sends = sends;
        let (from, replaced) = old.unwrap_or((self.transcript.turns.len(), Vec::new()));
        self.resplice(from, &replaced)
    }

    /// Shows a transcript folded elsewhere, such as a sub-agent's inside its parent's, and returns how the rows moved, if they did.
    pub fn show(&mut self, transcript: Transcript) -> Option<Change> {
        if transcript == self.transcript {
            return None;
        }
        self.fold = Fold::default();
        let old = std::mem::replace(&mut self.transcript, transcript);
        self.resplice(0, &old.turns)
    }

    /// Rebuilds the rows of turns from `from` on, which `old` held before, and the trailer under them.
    fn resplice(&mut self, from: usize, old: &[Turn]) -> Option<Change> {
        let keep = self.rows.partition_point(|r| r.turn().is_some_and(|turn| turn < from));
        let old_rows = self.rows.split_off(keep);
        rows_from(&mut self.rows, &self.transcript, from, self.sends.0, self.sends.1);
        let ids: Vec<String> = self.rows[keep..].iter().map(|r| r.id(&self.transcript)).collect();
        let (range, count) = splice(&self.ids[keep..], &ids);
        // Ids hold while a row's content changes (a tool finishing, text streaming in): the list measures those rows
        // afresh where they are, keeping their height until then, rather than replacing them.
        let first = (0..range.start).find(|&i| old_rows[i] != self.rows[keep + i] || !same_content(old, from, &self.transcript, &self.rows[keep + i]));
        let first = first.unwrap_or(range.start);
        self.ids.truncate(keep);
        self.ids.extend(ids);
        (first < range.end || count > 0).then_some((keep + first..keep + range.start, keep + range.start..keep + range.end, count))
    }

    /// Whether the row `id` shows open: the user's choice, else `default`.
    pub fn open(&self, id: &str, default: bool) -> bool {
        self.folds.get(id).copied().unwrap_or(default)
    }

    pub fn toggle(&mut self, id: &str, default: bool) {
        let open = self.open(id, default);
        self.folds.insert(id.into(), !open);
    }

    /// The fold over row `ix` when it shows shut, to open so the row shows; none while work going on holds it open.
    pub fn shut_fold_over(&self, ix: usize) -> Option<String> {
        let t = &self.transcript;
        let f = self.rows[ix].turn().and_then(|turn| crate::fold_of(t, turn)).filter(|&f| !crate::at_work_on(t, crate::folded(t, f)))?;
        Some(format!("{}#fold", t.turns[f].id)).filter(|id| !self.open(id, false))
    }

    /// The fold whose live line shows under the newest exchange, read off the trailer rather than the whole transcript.
    pub fn live_tail(&self) -> Option<usize> {
        match self.rows.last() {
            Some(Row::Live { turn }) => Some(*turn),
            _ => None,
        }
    }

    /// Whether row `ix` is a phase that shows shut.
    pub fn shut_phase(&self, ix: usize) -> bool {
        matches!(self.rows[ix], Row::Phase { .. }) && !self.open(&self.ids[ix], false)
    }
}

/// Whether `row`, of a turn from `from` on, shows what it showed when `old` held those turns.
fn same_content(old: &[Turn], from: usize, new: &Transcript, row: &Row) -> bool {
    let Some(turn) = row.turn() else { return true };
    let (Some(a), Some(b)) = (old.get(turn - from), new.turns.get(turn)) else { return false };
    match row {
        Row::Prompt { .. } => a.prompt == b.prompt,
        Row::Text { block, range, .. } => text(a, *block).get(range.clone()) == text(b, *block).get(range.clone()),
        Row::Phase { blocks, .. } => a.blocks.get(blocks.clone()) == b.blocks.get(blocks.clone()),
        Row::Tasks { block, .. } | Row::Question { block, .. } | Row::Compacted { block, .. } => a.blocks.get(*block) == b.blocks.get(*block),
        // One line, whatever the reply it heads does.
        Row::Fold { .. } | Row::Notice { .. } => true,
        Row::End { .. } => a.end == b.end,
        Row::Live { .. } | Row::Working | Row::Pending(_) => true,
    }
}

fn text(turn: &Turn, block: usize) -> &str {
    match turn.blocks.get(block) {
        Some(Block::Text(m) | Block::Plan(m)) => &m.text,
        _ => "",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{for_provider, rows};
    use agents::Item;
    use serde_json::{json, Value};

    fn item(seq: i64, v: Value) -> Item {
        let mut v = v;
        v["seq"] = json!(seq);
        v["id"] = json!(format!("i{seq}"));
        v["ts"] = json!(seq);
        serde_json::from_value(v).unwrap()
    }

    fn bash(seq: i64, id: &str, status: &str) -> Item {
        item(seq, json!({"kind": "tool", "call": {"toolUseId": id, "name": "Bash", "status": status, "output": "x\n".repeat(40), "detail": {"kind": "shell", "command": "make"}}}))
    }

    #[test]
    fn showing_a_row_opens_the_shut_fold_over_it_and_its_shut_phase_but_no_fold_work_holds_open() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let items = vec![item(1, json!({"kind": "user", "text": "go"})), bash(2, "t1", "ok"), item(3, json!({"kind": "result", "ok": true}))];
        s.update(backend, 1, &items, false, 0);
        let phase = s.rows.iter().position(|r| matches!(r, Row::Phase { .. })).unwrap();
        assert_eq!((s.shut_fold_over(phase).as_deref(), s.shut_phase(phase)), (Some("i1#fold"), true));
        s.toggle("i1#fold", false);
        s.toggle(&s.ids[phase].clone(), false);
        assert_eq!((s.shut_fold_over(phase), s.shut_phase(phase)), (None, false));

        let task = json!({"kind": "tool", "call": {"toolUseId": "tu", "name": "Task", "status": "running", "detail": {"kind": "task", "description": "audit"}}});
        let mut s = Session::default();
        s.update(backend, 1, &[item(1, json!({"kind": "user", "text": "go"})), item(2, task), item(3, json!({"kind": "result", "ok": true}))], false, 0);
        let agent = s.rows.iter().position(|r| matches!(r, Row::Phase { .. })).unwrap();
        assert_eq!(s.shut_fold_over(agent), None, "its sub-agent still at work holds the fold open");
    }

    #[test]
    fn a_session_refolds_only_when_the_revision_moves() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"}))];
        assert_eq!(s.update(backend, 1, &items, false, 0), Some((0..0, 0..0, 3)));
        assert_eq!(s.ids, vec!["i1", "i1#fold", "i1#live"]);
        items.push(item(2, json!({"kind": "assistant", "text": "on it"})));
        assert_eq!(s.update(backend, 1, &items, false, 0), None, "same revision, nothing refolded");
        assert_eq!(s.transcript.turns[0].blocks.len(), 0);
        assert_eq!(s.update(backend, 2, &items, false, 0), Some((2..2, 2..3, 2)), "the prompt and fold rows stay");
        assert_eq!(s.ids, vec!["i1", "i1#fold", "i1#0.0", "i1#live"]);
    }

    #[test]
    fn a_tool_finishing_replaces_its_turns_rows_though_their_ids_hold() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"})), bash(2, "t1", "running")];
        s.update(backend, 1, &items, false, 0);
        items[1] = bash(2, "t1", "ok");
        assert_eq!(s.update(backend, 2, &items, false, 0), Some((2..4, 4..4, 0)), "the prompt and fold stay, the phase and the live line under it are measured afresh");
        assert_eq!(s.update(backend, 3, &items, false, 0), None, "a refold that changes nothing splices nothing");
    }

    #[test]
    fn streaming_text_replaces_only_the_markdown_block_that_grew() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"})), item(2, json!({"kind": "assistant", "text": "a\n\nb"}))];
        s.update(backend, 1, &items, false, 0);
        items[1] = item(2, json!({"kind": "assistant", "text": "a\n\nbc"}));
        assert_eq!(s.update(backend, 2, &items, false, 0), Some((3..5, 5..5, 0)));
    }

    #[test]
    fn an_agent_with_no_items_shows_no_rows_and_no_working_row() {
        let mut s = Session::default();
        assert_eq!(s.update(for_provider("claude"), 1, &[], false, 0), None);
        assert!(s.rows.is_empty() && s.ids.is_empty());
    }

    #[test]
    fn a_streamed_reply_grows_in_one_row_until_its_end_follows_it() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"}))];
        s.update(backend, 1, &items, false, 0);
        assert_eq!(s.ids, ["i1", "i1#fold", "i1#live"]);
        let chunks = ["The", "The transcript", "The transcript jumped."];
        for (n, text) in chunks.into_iter().enumerate() {
            items.truncate(1);
            items.push(item(2, json!({"kind": "assistant", "text": text})));
            let splice = s.update(backend, 2 + n as u64, &items, false, 0);
            assert_eq!(s.ids, ["i1", "i1#fold", "i1#0.0", "i1#live"], "chunk {n}");
            assert_eq!(super::text(&s.transcript.turns[0], 0), text);
            if n > 0 {
                assert_eq!(splice, Some((2..4, 4..4, 0)), "only the growing reply and the live line under it are re-measured");
            }
        }
        items.push(item(3, json!({"kind": "result", "ok": true, "durationMs": 1200})));
        assert_eq!(s.update(backend, 9, &items, false, 0), Some((3..3, 3..4, 1)), "the end takes the live line's place");
        assert_eq!(s.ids, ["i1", "i1#fold", "i1#0.0", "i1#end"]);
    }

    #[test]
    fn a_pending_send_adds_its_row_without_a_refold() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let items = vec![item(1, json!({"kind": "user", "text": "go"})), item(2, json!({"kind": "result", "ok": true}))];
        s.update(backend, 1, &items, false, 0);
        s.outgoing.push("next".into(), false, 10);
        assert_eq!(s.update(backend, 1, &items, false, 10), Some((3..3, 3..3, 2)));
        assert_eq!(s.ids[3..], ["pending-0", "working"]);
    }

    #[test]
    fn a_send_not_delivered_drops_the_sending_trailer() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let items = vec![item(1, json!({"kind": "user", "text": "go"})), item(2, json!({"kind": "result", "ok": true}))];
        s.update(backend, 1, &items, false, 0);
        s.outgoing.push("next".into(), false, 10);
        s.update(backend, 1, &items, false, 10);
        assert_eq!(s.update(backend, 1, &items, false, 200_000), Some((4..4, 4..5, 0)));
        assert_eq!(s.ids[3..], ["pending-0"]);
    }

    #[test]
    fn a_queued_send_waits_off_the_transcript_until_its_turn_ends() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"}))];
        s.update(backend, 1, &items, false, 0);
        s.outgoing.push("next".into(), true, 10);
        assert_eq!(s.update(backend, 1, &items, false, 10), None, "the composer's tray shows it");
        items.push(item(2, json!({"kind": "result", "ok": true})));
        s.update(backend, 2, &items, false, 20);
        assert_eq!(s.ids[3..], ["pending-0", "working"]);
    }

    #[test]
    fn a_new_conversation_keeps_queued_sends_and_fails_none() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let old = vec![item(1, json!({"kind": "user", "text": "go"}))];
        s.update(backend, 1, &old, false, 0);
        s.outgoing.push("next".into(), true, 10);
        s.update(backend, 1, &old, false, 10);
        s.update(backend, 2, &[], false, 20);
        assert!(s.transcript.turns.is_empty() && !s.ids.contains(&"i1".to_string()), "the old conversation's rows are gone");
        let released: Vec<String> = s.outgoing.released().into_iter().map(|(_, text)| text).collect();
        assert_eq!(released, ["next"], "the new conversation is idle, so the queue goes on");
        let new = vec![item(5, json!({"kind": "user", "text": "next"}))];
        s.update(backend, 3, &new, false, 30);
        s.update(backend, 3, &new, false, 200_000);
        assert!(s.outgoing.list().is_empty(), "{:?}", s.outgoing.list());
    }

    #[test]
    fn a_shown_transcript_splices_only_when_it_changes() {
        let backend = for_provider("claude");
        let t = backend.transcript(&[item(1, json!({"kind": "user", "text": "go"})), item(2, json!({"kind": "assistant", "text": "on it"}))]);
        let mut s = Session::default();
        assert_eq!(s.show(t.clone()), Some((0..0, 0..0, 4)));
        assert_eq!(s.ids, vec!["i1", "i1#fold", "i1#0.0", "i1#live"]);
        assert_eq!(s.show(t), None);
    }

    #[test]
    fn toggling_a_row_flips_what_it_shows_from_its_default() {
        let mut s = Session::default();
        assert!(s.open("u1#p0", true));
        s.toggle("u1#p0", true);
        assert!(!s.open("u1#p0", true));
        s.toggle("u1#p0", true);
        assert!(s.open("u1#p0", false), "the user's choice outlasts the default");
        s.toggle("u1#fold", false);
        assert!(s.open("u1#fold", false));
    }

    /// Takes `item` in as `Agents` does: replacing the one with its id, then in seq order.
    fn upsert(items: &mut Vec<Item>, item: Item) {
        match items.iter_mut().find(|x| x.id == item.id) {
            Some(x) => *x = item,
            None => items.push(item),
        }
        items.sort_by_key(|x| x.seq);
    }

    /// The splice from rebuilding and comparing every row, as the session did before it refolded only the tail.
    fn full_splice(old: &Transcript, new: &Transcript) -> Option<Change> {
        let (old_rows, new_rows) = (rows(old, 0, false), rows(new, 0, false));
        let ids = |t: &Transcript, rows: &[Row]| rows.iter().map(|r| r.id(t)).collect::<Vec<_>>();
        let (range, count) = splice(&ids(old, &old_rows), &ids(new, &new_rows));
        let first = (0..range.start).find(|&i| old_rows[i] != new_rows[i] || !same_content(&old.turns, 0, new, &new_rows[i])).unwrap_or(range.start);
        (first < range.end || count > 0).then_some((first..range.start, range, count))
    }

    /// Feeds the session one item at a time, checking each refold against folding everything afresh.
    fn replay(updates: Vec<Value>) {
        let backend = for_provider("claude");
        let (mut s, mut items, mut shown) = (Session::default(), Vec::new(), Transcript::default());
        for (n, v) in updates.into_iter().enumerate() {
            upsert(&mut items, serde_json::from_value(v).unwrap());
            let splice = s.update(backend, n as u64 + 1, &items, false, 0);
            let full = backend.transcript(&items);
            let rows = rows(&full, 0, false);
            assert_eq!(s.transcript, full, "update {n}");
            assert_eq!(s.rows, rows, "update {n}");
            assert_eq!(s.ids, rows.iter().map(|r| r.id(&full)).collect::<Vec<_>>(), "update {n}");
            assert_eq!(splice, full_splice(&shown, &full), "update {n}");
            shown = full;
        }
    }

    /// A scenario's items as pocketd streams them, ids prefixed: text grows in chunks, and a finished call
    /// first runs until the next item lands.
    fn streamed(items: Vec<Value>, prefix: &str, seq: &mut i64) -> Vec<Value> {
        let mut out = Vec::new();
        let mut finishing: Option<Value> = None;
        for mut v in items {
            *seq += 1;
            v["seq"] = json!(*seq);
            v["ts"] = json!(*seq);
            v["id"] = json!(format!("{prefix}{}", v["id"].as_str().unwrap()));
            for key in ["parentToolUseId"] {
                if let Some(p) = v[key].as_str() {
                    v[key] = json!(format!("{prefix}{p}"));
                }
            }
            if let Some(id) = v["call"]["toolUseId"].as_str() {
                v["call"]["toolUseId"] = json!(format!("{prefix}{id}"));
            }
            let text = v["text"].as_str().unwrap_or_default().to_string();
            let cuts: Vec<usize> = text.match_indices(' ').map(|(i, _)| i).step_by(7).collect();
            for cut in cuts {
                let mut partial = v.clone();
                partial["text"] = json!(text[..cut]);
                out.push(partial);
            }
            if v["call"]["status"].as_str().is_some_and(|s| s != "running") {
                let mut running = v.clone();
                running["call"]["status"] = json!("running");
                running["call"]["output"] = Value::Null;
                out.push(running);
                out.extend(finishing.replace(v));
            } else {
                out.push(v);
                out.extend(finishing.take());
            }
        }
        out.extend(finishing);
        out
    }

    fn scenarios() -> Vec<(String, Vec<Value>)> {
        let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../../../../.ui-review/fixture/chat");
        let mut all: Vec<_> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().path())
            .map(|p| (p.file_stem().unwrap().to_string_lossy().into_owned(), serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()))
            .collect();
        all.sort_by(|a, b| a.0.cmp(&b.0));
        assert!(all.len() >= 10, "{dir}");
        all
    }

    #[test]
    fn refolding_each_fixture_scenario_as_it_streams_matches_folding_it_whole() {
        let mut chained = Vec::new();
        let mut chained_seq = 0;
        for (name, items) in scenarios() {
            replay(streamed(items.clone(), "", &mut 0));
            chained.extend(streamed(items, &format!("{name}-"), &mut chained_seq));
        }
        replay(chained);
    }

    /// A long session as pocketd streams it: replies grow in place, calls finish later (some after the next prompt,
    /// some as a new item), sub-agents two deep report into running calls, and now and then an item lands between older ones.
    fn generated(count: usize) -> Vec<Value> {
        let mut rng = 0x2545_f491_4f6c_dd1du64;
        let mut roll = move |n: usize| {
            rng ^= rng << 13;
            rng ^= rng >> 7;
            rng ^= rng << 17;
            (rng % n as u64) as usize
        };
        let (mut out, mut running, mut agents): (Vec<Value>, Vec<Value>, Vec<String>) = (Vec::new(), Vec::new(), Vec::new());
        for n in 0..count {
            let (id, seq) = (format!("g{n}"), 10 * n as i64 + 10);
            let mut v = match roll(100) {
                0..8 => json!({"kind": "user", "text": format!("prompt {n}")}),
                8..10 => json!({"kind": "result", "ok": roll(4) > 0, "error": "interrupted"}),
                10..30 => json!({"kind": "assistant", "text": format!("reply {n}\n\n- one\n- two\n\nthe end of {n}")}),
                30..35 => json!({"kind": "thinking", "text": format!("thought {n}")}),
                35..55 => {
                    let (name, detail) = [("Bash", json!({"kind": "shell", "command": "make"})), ("Read", json!({"kind": "read", "path": "a.rs"})), ("Task", json!({"kind": "task", "description": "audit"}))][roll(3)].clone();
                    if name == "Task" {
                        agents.push(format!("tu{n}"));
                    }
                    json!({"kind": "tool", "call": {"toolUseId": format!("tu{n}"), "name": name, "status": "running", "detail": detail}})
                }
                55..70 if !running.is_empty() => {
                    let ix = if roll(5) == 0 { roll(running.len()) } else { running.len() - 1 };
                    let mut call = running.remove(ix);
                    agents.retain(|a| *a != call["call"]["toolUseId"]);
                    call["call"]["status"] = json!(["ok", "error"][roll(2)]);
                    call["call"]["output"] = json!(format!("out {n}"));
                    if roll(2) == 0 {
                        out.push(call);
                        continue;
                    }
                    json!({"kind": "tool", "call": call["call"]})
                }
                70..82 if !agents.is_empty() => {
                    let parent = agents[if roll(5) == 0 { roll(agents.len()) } else { agents.len() - 1 }].clone();
                    let mut child = if roll(5) == 0 {
                        agents.push(format!("tu{n}"));
                        json!({"kind": "tool", "call": {"toolUseId": format!("tu{n}"), "name": "Task", "status": "running", "detail": {"kind": "task", "description": "deeper"}}})
                    } else {
                        json!({"kind": "assistant", "text": format!("sub {n}")})
                    };
                    child["parentToolUseId"] = json!(parent);
                    child
                }
                82..84 => json!({"kind": "tasks", "items": [{"text": format!("todo {n}"), "status": "in_progress"}]}),
                84..85 => json!({"kind": "compact", "trigger": "auto"}),
                85..87 => json!({"kind": "assistant", "text": "  "}),
                87..89 => json!({"kind": "assistant", "text": format!("orphan {n}"), "parentToolUseId": "gone"}),
                89..91 => json!({"kind": "assistant", "text": format!("late {n}"), "seq": 10 * roll(n + 1) as i64 + 5}),
                _ => json!({"kind": "assistant", "text": format!("short {n}")}),
            };
            v["id"] = json!(id);
            v["ts"] = json!(seq);
            if v["seq"].is_null() {
                v["seq"] = json!(seq);
            }
            if v["kind"] == "tool" && v["call"]["status"] == "running" {
                running.push(v.clone());
            }
            if v["kind"] == "assistant" && v.get("parentToolUseId").is_none() {
                let text = v["text"].as_str().unwrap().to_string();
                for cut in text.match_indices('\n').map(|(i, _)| i) {
                    let mut partial = v.clone();
                    partial["text"] = json!(text[..cut]);
                    out.push(partial);
                }
            }
            out.push(v);
        }
        out
    }

    #[test]
    fn refolding_a_five_thousand_item_stream_item_by_item_matches_folding_it_whole() {
        let updates = generated(5000);
        assert!(updates.len() > 5000);
        replay(updates);
    }

    /// `turns` finished turns of a prompt, thinking, a reply, three tools, a closing reply and a result: 8 items each.
    fn history(turns: usize) -> Vec<Item> {
        let mut items = Vec::new();
        let mut seq = 0;
        let mut next = || {
            seq += 1;
            seq
        };
        let long: String = (0..12).map(|i| format!("## Step {i}\n\nA paragraph about it.\n\n- one\n- two\n\n")).collect();
        for turn in 0..turns {
            items.push(item(next(), json!({"kind": "user", "text": format!("turn {turn}")})));
            items.push(item(next(), json!({"kind": "thinking", "text": "considering ".repeat(40)})));
            items.push(item(next(), json!({"kind": "assistant", "text": if turn % 5 == 0 { long.clone() } else { "On it.".into() }})));
            items.push(bash(next(), &format!("b{turn}"), "ok"));
            items.push(item(next(), json!({"kind": "tool", "call": {"toolUseId": format!("r{turn}"), "name": "Read", "status": "ok", "output": "line\n".repeat(200), "detail": {"kind": "read", "path": "src/lib.rs"}}})));
            let lines: Vec<Value> = (0..20).map(|n| json!({"number": n, "kind": if n % 2 == 0 { "add" } else { "del" }, "text": "let x = 1;"})).collect();
            items.push(item(next(), json!({"kind": "tool", "call": {"toolUseId": format!("e{turn}"), "name": "Edit", "status": "ok", "detail": {"kind": "edit", "path": "src/lib.rs", "diff": {"lines": lines, "additions": 10, "deletions": 10}}}})));
            items.push(item(next(), json!({"kind": "assistant", "text": "Done."})));
            items.push(item(next(), json!({"kind": "result", "ok": true, "durationMs": 1200})));
        }
        items
    }

    /// p50, p95 and max of `update` while a reply streams into a new turn after `n` items of history, in µs.
    fn streaming(n: usize) -> (u128, u128, u128) {
        let backend = for_provider("claude");
        let mut items = history(n / 8);
        let mut s = Session::default();
        s.update(backend, 1, &items, false, 0);
        let seq = items.len() as i64;
        items.push(item(seq + 1, json!({"kind": "user", "text": "stream"})));
        items.push(item(seq + 2, json!({"kind": "assistant", "text": ""})));
        let mut took = Vec::new();
        let mut text = String::new();
        for chunk in 0..200 {
            text.push_str(if chunk % 20 == 19 { "end.\n\n" } else { "word " });
            items.last_mut().unwrap().text = text.clone();
            let start = std::time::Instant::now();
            std::hint::black_box(s.update(backend, 2 + chunk, &items, false, 0));
            took.push(start.elapsed().as_micros());
        }
        took.sort();
        (took[took.len() / 2], took[took.len() * 95 / 100], took[took.len() - 1])
    }

    /// `cargo test --release -p chat -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn a_reply_streaming_after_twelve_hundred_or_five_thousand_items_refolds_well_under_two_ms() {
        for n in [1200, 5000] {
            let (p50, p95, max) = streaming(n);
            eprintln!("{n} items: update p50 {p50}µs p95 {p95}µs max {max}µs");
            assert!(p50 < 2000 && p95 < 2000, "{n}: {p50}µs/{p95}µs");
        }
    }

    /// `cargo test --release -p chat -- --ignored`
    #[test]
    #[ignore]
    fn folding_twelve_hundred_items_takes_well_under_two_ms() {
        let items = history(150);
        assert_eq!(items.len(), 1200);
        let backend = for_provider("claude");
        let mut s = Session::default();
        s.update(backend, 1, &items, false, 0);
        let runs = 50;
        let start = std::time::Instant::now();
        for revision in 2..2 + runs {
            std::hint::black_box(s.update(backend, revision, &items, false, 0));
        }
        let each = start.elapsed() / runs as u32;
        let start = std::time::Instant::now();
        s.update(backend, 1 + runs, &items, false, 0);
        let idle = start.elapsed();
        eprintln!("update: {each:?} per refold, {idle:?} unchanged");
        assert!(each.as_micros() < 2000, "{each:?}");
    }

    #[test]
    fn a_pocketd_restart_mid_turn_holds_a_queued_send() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let working = vec![item(1, json!({"kind": "user", "text": "long job"})), item(2, json!({"kind": "assistant", "text": "on it"}))];
        s.update(backend, 1, &working, true, 0);
        s.outgoing.push("next".into(), true, 10);
        s.update(backend, 1, &working, true, 10);
        // pocketd restarted: its resets went back to 0, so the held items were dropped until it sends them again.
        s.update(backend, 2, &[], true, 20);
        assert!(s.outgoing.released().is_empty(), "the agent still works");
        s.update(backend, 3, &working, true, 30);
        assert!(s.outgoing.released().is_empty());
    }

    #[test]
    fn a_command_reads_as_work_until_the_timeline_moves() {
        let backend = for_provider("claude");
        let mut s = Session::default();
        let mut items = vec![item(1, json!({"kind": "user", "text": "go"})), item(2, json!({"kind": "result", "ok": true}))];
        s.update(backend, 1, &items, false, 0);
        s.outgoing.push("/compact".into(), false, 10);
        s.update(backend, 1, &items, true, 20);
        assert!(crate::busy(&s.transcript, &s.outgoing, true));
        items.push(item(3, json!({"kind": "compact", "trigger": "manual"})));
        s.update(backend, 2, &items, true, 4_000);
        assert!(!crate::busy(&s.transcript, &s.outgoing, true), "the compaction shows; status trails it");
    }
}
