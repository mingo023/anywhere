//! A Workflow call: the script Claude runs in the background, and the agents it starts in its phases.

use crate::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript};
use serde_json::Value;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Workflow {
    /// What its script's `meta` calls it, else the saved workflow it ran; empty when neither says.
    pub name: String,
    /// The phases its `meta` lists, in order.
    pub phases: Vec<Phase>,
    /// Its agents' calls, each a `ToolKind::Agent` naming its phase.
    pub transcript: Transcript,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Phase {
    pub title: String,
    pub detail: String,
}

/// A phase and the agents running in it, in the order they started.
#[derive(Debug, PartialEq)]
pub struct Group<'a> {
    pub title: &'a str,
    pub detail: &'a str,
    pub agents: Vec<(&'a ToolCall, &'a SubAgent)>,
}

/// Where a workflow's phase stands.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stage {
    Pending,
    Active,
    Done,
}

impl Group<'_> {
    pub fn stage(&self) -> Stage {
        match () {
            _ if self.agents.is_empty() => Stage::Pending,
            _ if self.agents.iter().any(|(c, _)| c.status == ToolStatus::Running) => Stage::Active,
            _ => Stage::Done,
        }
    }
}

/// The phase it's in, as its index: the first with an agent at work, else the last any agent ran in.
pub fn current(groups: &[Group]) -> Option<usize> {
    groups.iter().position(|g| g.stage() == Stage::Active).or_else(|| groups.iter().rposition(|g| g.stage() == Stage::Done))
}

/// "Phase 2/3 · Implement": the phase it's in.
pub fn phase_line(groups: &[Group]) -> Option<String> {
    let at = current(groups)?;
    let title = groups[at].title;
    Some(if title.is_empty() { format!("Phase {}/{}", at + 1, groups.len()) } else { format!("Phase {}/{} · {title}", at + 1, groups.len()) })
}

/// What phase `at`, which no agent has started in, waits on.
pub fn waits_on(groups: &[Group], at: usize) -> String {
    match at.checked_sub(1).map(|p| groups[p].title) {
        Some(prev) if !prev.is_empty() => format!("Starts when {prev} finishes"),
        _ => "Not started".into(),
    }
}

impl Workflow {
    /// From a Workflow call's input. `meta` is a JavaScript literal, not JSON, so only its strings are read.
    pub fn from_input(input: &Value) -> Self {
        let script = input.get("script").and_then(Value::as_str).unwrap_or_default();
        let meta = after(script, "meta").and_then(|rest| enclosed(rest, '{', '}')).unwrap_or_default();
        let (phases, rest) = match after(meta, "phases").and_then(|rest| enclosed(rest, '[', ']')) {
            Some(list) => (phases(list), meta.replacen(list, "", 1)),
            None => (Vec::new(), meta.to_string()),
        };
        let saved = || input.get("name").and_then(Value::as_str).map(str::to_string);
        Workflow { name: string(&rest, "name").or_else(saved).unwrap_or_default(), phases, transcript: Transcript::default() }
    }

    /// "Workflow measure", or just "Workflow" without a name.
    pub fn label(&self) -> String {
        if self.name.is_empty() { "Workflow".into() } else { format!("Workflow {}", self.name) }
    }

    /// How many of its agents finished, of all it started.
    pub fn tally(&self) -> (usize, usize) {
        self.agents().fold((0, 0), |(done, all), (c, _)| (done + usize::from(c.status != ToolStatus::Running), all + 1))
    }

    /// Its agents' calls, in the order they started.
    pub fn agents(&self) -> impl Iterator<Item = (&ToolCall, &SubAgent)> {
        self.transcript.turns.iter().flat_map(|t| &t.blocks).filter_map(|b| match b {
            Block::Tool(c @ ToolCall { kind: ToolKind::Agent(a), .. }) => Some((c, a)),
            _ => None,
        })
    }

    /// Its phases with their agents: the ones `meta` lists, then any its agents name that it doesn't, as they first do.
    pub fn groups(&self) -> Vec<Group<'_>> {
        let mut groups: Vec<Group> = self.phases.iter().map(|p| Group { title: &p.title, detail: &p.detail, agents: Vec::new() }).collect();
        for (call, agent) in self.agents() {
            let at = match groups.iter().position(|g| g.title == agent.phase) {
                Some(at) => at,
                None => {
                    groups.push(Group { title: &agent.phase, detail: "", agents: Vec::new() });
                    groups.len() - 1
                }
            };
            groups[at].agents.push((call, agent));
        }
        groups
    }
}

fn phases(list: &str) -> Vec<Phase> {
    let mut out = Vec::new();
    let mut rest = list;
    while let Some(entry) = enclosed(rest, '{', '}') {
        if let Some(title) = string(entry, "title") {
            out.push(Phase { title, detail: string(entry, "detail").unwrap_or_default() });
        }
        rest = &rest[entry.as_ptr() as usize - rest.as_ptr() as usize + entry.len()..];
    }
    out
}

/// What follows the first `key` in `s` that stands as a key: a whole word, quoted or not, before a colon or `=`.
fn after<'a>(s: &'a str, key: &str) -> Option<&'a str> {
    let mut from = 0;
    while let Some(i) = s[from..].find(key).map(|i| i + from) {
        from = i + key.len();
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '$');
        if word(s[..i].chars().next_back()) || word(s[from..].chars().next()) {
            continue;
        }
        let rest = s[from..].trim_start_matches(['"', '\'']).trim_start();
        if let Some(rest) = rest.strip_prefix(':').or_else(|| rest.strip_prefix('=')) {
            return Some(rest);
        }
    }
    None
}

/// The string literal that `key` holds in `s`.
fn string(s: &str, key: &str) -> Option<String> {
    let rest = after(s, key)?.trim_start();
    let quote = rest.chars().next().filter(|c| matches!(c, '"' | '\'' | '`'))?;
    let mut out = String::new();
    let mut chars = rest[1..].chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c if c == quote => return Some(out),
            c => out.push(c),
        }
    }
    None
}

/// The first `open`…`close` span in `s`, brackets included, skipping any inside strings.
fn enclosed(s: &str, open: char, close: char) -> Option<&str> {
    let start = s.find(open)?;
    let (mut depth, mut quote, mut escaped) = (0, None, false);
    for (i, c) in s[start..].char_indices() {
        match (quote, c) {
            (Some(_), _) if escaped => escaped = false,
            (Some(_), '\\') => escaped = true,
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'' | '`') => quote = Some(c),
            (None, c) if c == open => depth += 1,
            (None, c) if c == close => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..start + i + c.len_utf8()]);
                }
            }
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Turn;
    use serde_json::json;

    fn titles(w: &Workflow) -> Vec<(&str, &str)> {
        w.phases.iter().map(|p| (p.title.as_str(), p.detail.as_str())).collect()
    }

    #[test]
    fn a_scripts_meta_names_the_workflow_and_its_phases_in_order() {
        let script = r#"export const meta = {
  name: 'measure',
  description: "Time the build, then judge it",
  phases: [
    { title: 'Instrument', detail: "Add timings, {braces} and 'quotes' too" },
    { "title": "Judge" },
  ],
}
const r = await agent('x', { phase: 'Instrument', name: 'not this' })"#;
        let w = Workflow::from_input(&json!({ "script": script }));
        assert_eq!(w.name, "measure");
        assert_eq!(titles(&w), [("Instrument", "Add timings, {braces} and 'quotes' too"), ("Judge", "")]);
    }

    #[test]
    fn a_workflow_without_a_readable_meta_keeps_its_saved_name_and_no_phases() {
        let w = Workflow::from_input(&json!({ "name": "review-changes" }));
        assert_eq!((w.name.as_str(), w.phases.len()), ("review-changes", 0));
        let w = Workflow::from_input(&json!({ "script": "export const meta = { name: 'cut', phases: [ { title: 'A'" }));
        assert_eq!((w.name.as_str(), w.phases.len()), ("", 0), "an unclosed meta reads as none");
        let w = Workflow::from_input(&json!({ "script": "const metadata = { name: 'no' }; const subtitle = 'x'" }));
        assert_eq!(w, Workflow::default(), "only a whole word is a key");
    }

    #[test]
    fn agents_group_under_their_phase_and_a_phase_meta_lacks_comes_after_as_first_named() {
        let agent = |id: &str, phase: &str| {
            Block::Tool(ToolCall {
                id: id.into(),
                name: "Agent".into(),
                kind: ToolKind::Agent(SubAgent { phase: phase.into(), ..Default::default() }),
                status: ToolStatus::Running,
                output: None,
                duration_ms: None,
                at: 0,
            })
        };
        let w = Workflow {
            phases: vec![Phase { title: "Plan".into(), detail: "d".into() }, Phase { title: "Build".into(), detail: String::new() }],
            transcript: Transcript {
                turns: vec![Turn { blocks: vec![agent("a", "Build"), agent("b", "Extra"), agent("c", "Plan"), agent("d", "Build")], ..Turn::default() }],
            },
            ..Workflow::default()
        };
        let got: Vec<(&str, Vec<&str>)> = w.groups().iter().map(|g| (g.title, g.agents.iter().map(|(c, _)| c.id.as_str()).collect())).collect();
        assert_eq!(got, [("Plan", vec!["c"]), ("Build", vec!["a", "d"]), ("Extra", vec!["b"])]);
    }

    fn workflow(phases: &[&str], agents: &[(&str, ToolStatus)]) -> Workflow {
        let blocks = agents
            .iter()
            .enumerate()
            .map(|(i, (phase, status))| {
                let kind = ToolKind::Agent(SubAgent { phase: phase.to_string(), ..Default::default() });
                Block::Tool(ToolCall { id: format!("a{i}"), name: "Agent".into(), kind, status: *status, output: None, duration_ms: None, at: 0 })
            })
            .collect();
        Workflow {
            phases: phases.iter().map(|t| Phase { title: t.to_string(), detail: String::new() }).collect(),
            transcript: Transcript { turns: vec![Turn { blocks, ..Turn::default() }] },
            ..Workflow::default()
        }
    }

    #[test]
    fn a_phase_is_pending_until_an_agent_starts_in_it_and_done_once_none_still_runs() {
        let w = workflow(&["Plan", "Build", "Judge"], &[("Plan", ToolStatus::Ok), ("Build", ToolStatus::Running), ("Build", ToolStatus::Ok)]);
        let groups = w.groups();
        assert_eq!(groups.iter().map(Group::stage).collect::<Vec<_>>(), [Stage::Done, Stage::Active, Stage::Pending]);
        assert_eq!(phase_line(&groups).as_deref(), Some("Phase 2/3 · Build"));
        assert_eq!(waits_on(&groups, 2), "Starts when Build finishes");
        assert_eq!(waits_on(&groups, 0), "Not started");
        assert_eq!(w.tally(), (2, 3));
    }

    #[test]
    fn once_no_agent_runs_the_phase_it_is_in_is_the_last_one_any_ran_in() {
        let w = workflow(&["Plan", "Build", "Judge"], &[("Plan", ToolStatus::Ok), ("Build", ToolStatus::Failed)]);
        assert_eq!(current(&w.groups()), Some(1));
        assert_eq!(current(&workflow(&["Plan"], &[]).groups()), None, "nothing started yet");
    }

    #[test]
    fn a_workflow_without_a_name_is_labelled_just_workflow() {
        assert_eq!(Workflow::default().label(), "Workflow");
        assert_eq!(Workflow { name: "measure".into(), ..Workflow::default() }.label(), "Workflow measure");
    }
}
