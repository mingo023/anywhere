//! A prompt's markdown as it is typed: what of it to style, and the edits that keep a list going, format a selection
//! and take a paste. The text stays markdown, as the agent reads it.

use crate::marks::{code, in_code, Code};
use std::ops::Range;

/// How a span of a prompt reads.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Style {
    Bold,
    Italic,
    Strike,
    Link,
    /// Markdown's own characters, which show dimmed: emphasis marks, a link's brackets and address, a quote's `>`.
    Syntax,
    /// What a fenced block holds between its fences.
    Code,
}

/// A change to a prompt: `text` in place of `range`, then `select` selected.
#[derive(Debug, PartialEq)]
pub struct Edit {
    pub range: Range<usize>,
    pub text: String,
    pub select: Range<usize>,
}

/// A paste of more lines than this reads better as a file than in the prompt.
const LONG_PASTE_LINES: usize = 80;
/// Or of more bytes than this.
const LONG_PASTE_BYTES: usize = 8000;

const INDENT: &str = "  ";
const EMPHASIS: [(&str, Style); 4] = [("**", Style::Bold), ("~~", Style::Strike), ("*", Style::Italic), ("_", Style::Italic)];

#[derive(Clone, Copy, Debug, PartialEq)]
enum Marker {
    Bullet(char),
    /// The number and what closes it, `.` or `)`.
    Number(u64, char),
    Quote,
}

/// A line that opens a list item or a quote.
#[derive(Debug, PartialEq)]
struct Item<'a> {
    indent: &'a str,
    marker: Marker,
    /// The marker's length, without the space after it.
    width: usize,
    /// A checklist item's box, and whether it's ticked.
    check: Option<bool>,
    /// Where the line's own text starts.
    body: usize,
}

fn list_item(line: &str) -> Option<Item<'_>> {
    let rest = line.trim_start_matches([' ', '\t']);
    let indent = &line[..line.len() - rest.len()];
    let (marker, width) = match rest.chars().next()? {
        c @ ('-' | '*' | '+') => (Marker::Bullet(c), 1),
        '>' => (Marker::Quote, 1),
        c if c.is_ascii_digit() => {
            let digits = rest.find(|c: char| !c.is_ascii_digit())?;
            let close = rest[digits..].chars().next().filter(|c| matches!(c, '.' | ')'))?;
            (Marker::Number(rest[..digits].parse().ok()?, close), digits + 1)
        }
        _ => return None,
    };
    let after = rest[width..].strip_prefix(' ')?;
    let body = indent.len() + width + 1;
    let ticked = match after.get(..3) {
        Some("[ ]") => Some(false),
        Some("[x]" | "[X]") => Some(true),
        _ => None,
    };
    let check = ticked.filter(|_| matches!(marker, Marker::Bullet(_)) && (after.len() == 3 || after[3..].starts_with(' ')));
    let body = body + check.map_or(0, |_| (after.len()).min(4));
    Some(Item { indent, marker, width, check, body })
}

impl Item<'_> {
    /// What opens the item after this one.
    fn next(&self) -> String {
        let marker = match self.marker {
            Marker::Bullet(c) => c.to_string(),
            Marker::Number(n, close) => format!("{}{close}", n.saturating_add(1)),
            Marker::Quote => ">".into(),
        };
        format!("{}{marker} {}", self.indent, if self.check.is_some() { "[ ] " } else { "" })
    }
}

/// The line `at` is on.
fn line_at(text: &str, at: usize) -> Range<usize> {
    let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
    start..text[at..].find('\n').map_or(text.len(), |i| at + i)
}

/// Whether the line `at` is on is inside a fenced block, an unclosed one included, as it may be the block being typed.
fn in_block(text: &str, at: usize) -> bool {
    text[..line_at(text, at).start].matches("```").count() % 2 == 1
}

/// The lines around `at` that make one list: its items, and the deeper lines they hold.
fn list_at(text: &str, at: usize) -> Range<usize> {
    let listed = |line: &str| list_item(line).is_some() || (line.starts_with([' ', '\t']) && !line.trim().is_empty());
    let mut list = line_at(text, at);
    while list.start > 0 {
        let above = line_at(text, list.start - 1);
        if !listed(&text[above.clone()]) {
            break;
        }
        list.start = above.start;
    }
    while list.end < text.len() {
        let below = line_at(text, list.end + 1);
        if !listed(&text[below.clone()]) {
            break;
        }
        list.end = below.end;
    }
    list
}

/// The indent of a list's outermost items.
fn list_base(text: &str, list: Range<usize>) -> usize {
    text[list].lines().filter_map(list_item).map(|item| item.indent.len()).min().unwrap_or(0)
}

/// A list's lines with its numbered items counted on at each depth, and `points` moved with the lines they are on; one
/// past the list moves with its end. An item follows the one before it at its depth; one that opens a depth keeps its
/// number, or starts at 1 when it is one of the lines `restart`.
fn renumbered(list: &str, restart: &[usize], points: [usize; 2]) -> (String, [usize; 2]) {
    let mut out = String::new();
    // The last number at each depth still open, by its indent.
    let mut open: Vec<(usize, u64)> = Vec::new();
    let mut moved = points;
    let mut start = 0;
    for (i, line) in list.split_inclusive('\n').enumerate() {
        let body = line.trim_end_matches('\n');
        let begins = out.len();
        let (mut marker_end, mut grown) = (0, 0);
        match list_item(body) {
            Some(Item { indent, marker: Marker::Number(n, close), width, .. }) => {
                let depth = indent.len();
                open.retain(|(d, _)| *d <= depth);
                let n = match open.last() {
                    Some((d, last)) if *d == depth => last.saturating_add(1),
                    _ if restart.contains(&i) => 1,
                    _ => n,
                };
                open.retain(|(d, _)| *d < depth);
                open.push((depth, n));
                let marker = format!("{n}{close}");
                (marker_end, grown) = (depth + width, marker.len() as isize - width as isize);
                out.push_str(&format!("{indent}{marker}{}", &line[depth + width..]));
            }
            // A bullet or a quote ends the count at its depth.
            Some(Item { indent, .. }) => {
                open.retain(|(d, _)| *d < indent.len());
                out.push_str(line);
            }
            None => out.push_str(line),
        }
        for (point, moved) in points.iter().zip(&mut moved) {
            if (start..=start + body.len()).contains(point) {
                let column = point - start;
                *moved = begins + if column >= marker_end { column.wrapping_add_signed(grown) } else { column.min(marker_end.wrapping_add_signed(grown)) };
            }
        }
        start += line.len();
    }
    let end = out.len();
    let moved = [0, 1].map(|i| if points[i] > list.len() { end + points[i] - list.len() } else { moved[i].min(end) });
    (out, moved)
}

/// The bytes `a` and `b` share at their start.
fn shared_prefix(a: impl Iterator<Item = char>, b: impl Iterator<Item = char>) -> usize {
    a.zip(b).take_while(|(x, y)| x == y).map(|(x, _)| x.len_utf8()).sum()
}

/// The edit that leaves `changed`, renumbered, where the list at `list` was; `select` is in `changed`'s terms. Only
/// what differs is replaced, so the rest of the list keeps what the input holds on it.
fn relisted(text: &str, list: Range<usize>, changed: &str, restart: &[usize], select: Range<usize>) -> Edit {
    let (new, [start, end]) = renumbered(changed, restart, [select.start, select.end]);
    let old = &text[list.clone()];
    let lead = shared_prefix(old.chars(), new.chars());
    let tail = shared_prefix(old[lead..].chars().rev(), new[lead..].chars().rev());
    Edit { range: list.start + lead..list.end - tail, text: new[lead..new.len() - tail].into(), select: list.start + start..list.start + end }
}

/// A list just started, by `typed` ending in the space after its marker at a line's start, sits a step in from the
/// text around it. None under another item, whose list it is already part of, and for a quote.
pub fn list_started(text: &str, typed: Range<usize>, selection: Range<usize>) -> Option<Edit> {
    let at = typed.end;
    if typed.is_empty() || !text.get(..at).is_some_and(|t| t.ends_with(' ')) || selection != (at..at) || in_block(text, at) {
        return None;
    }
    let line = line_at(text, at);
    let item = list_item(&text[line.clone()])?;
    if !item.indent.is_empty() || item.marker == Marker::Quote || at != line.start + item.body {
        return None;
    }
    if line.start > 0 && list_item(&text[line_at(text, line.start - 1)]).is_some() {
        return None;
    }
    let at = at + INDENT.len();
    Some(Edit { range: line.start..line.start, text: INDENT.into(), select: at..at })
}

/// A new line in a list opens its next item, the numbers after it counting on. One in an empty item ends the list, or
/// only its own depth when it is nested under another. None outside a list, where a new line is only that.
pub fn newline(text: &str, selection: Range<usize>) -> Option<Edit> {
    let at = selection.start;
    if !selection.is_empty() || in_block(text, at) {
        return None;
    }
    let line = line_at(text, at);
    let item = list_item(&text[line.clone()])?;
    if at < line.start + item.body {
        return None;
    }
    let list = list_at(text, at);
    if text[line.start + item.body..line.end].trim().is_empty() {
        if item.indent.len() > list_base(text, list) {
            return indent(text, selection, true);
        }
        return Some(Edit { range: line.clone(), text: String::new(), select: line.start..line.start });
    }
    let lead = format!("\n{}", item.next());
    let cursor = at + lead.len() - list.start;
    Some(relisted(text, list.clone(), &format!("{}{lead}{}", &text[list.start..at], &text[at..list.end]), &[], cursor..cursor))
}

/// Tab nests the list items the selection is on a level, and Shift-Tab brings them back; a numbered item takes its
/// number from where it lands, and those after it count on. None outside a list, or with nothing to bring back.
pub fn indent(text: &str, selection: Range<usize>, out: bool) -> Option<Edit> {
    if in_block(text, selection.start) {
        return None;
    }
    let first = line_at(text, selection.start);
    list_item(&text[first.clone()])?;
    let list = list_at(text, selection.start);
    let picked = first.start..=line_at(text, selection.end.min(list.end)).start;
    let (mut changed, mut moves, mut restart) = (String::new(), Vec::new(), Vec::new());
    let mut start = list.start;
    for (i, line) in text[list.clone()].split_inclusive('\n').enumerate() {
        let item = list_item(line.trim_end_matches('\n')).filter(|_| picked.contains(&start));
        let cut = match &item {
            Some(item) if out && item.indent.starts_with('\t') => 1,
            Some(item) if out => item.indent.len().min(INDENT.len()),
            _ => 0,
        };
        match &item {
            Some(item) if !out || cut > 0 => {
                changed.push_str(if out { &line[cut..] } else { INDENT });
                if !out {
                    changed.push_str(line);
                }
                moves.push((start, cut));
                if matches!(item.marker, Marker::Number(..)) {
                    restart.push(i);
                }
            }
            _ => changed.push_str(line),
        }
        start += line.len();
    }
    if moves.is_empty() {
        return None;
    }
    let moved = |at: usize| {
        let shifted = moves.iter().filter(|(line, _)| *line <= at).fold(at as isize, |at, &(line, cut)| match out {
            false => at + INDENT.len() as isize,
            true => at - cut.min(at.max(0) as usize - line) as isize,
        });
        shifted as usize - list.start
    };
    Some(relisted(text, list.clone(), &changed, &restart, moved(selection.start)..moved(selection.end)))
}

/// Wraps the selection in `mark`, or unwraps it when it already is.
pub fn toggle(text: &str, selection: Range<usize>, mark: &str) -> Edit {
    let (before, picked, after) = (&text[..selection.start], &text[selection.clone()], &text[selection.end..]);
    let n = mark.len();
    if before.ends_with(mark) && after.starts_with(mark) {
        return Edit { range: selection.start - n..selection.end + n, text: picked.into(), select: selection.start - n..selection.end - n };
    }
    if picked.len() >= 2 * n && picked.starts_with(mark) && picked.ends_with(mark) {
        let inner = &picked[n..picked.len() - n];
        return Edit { range: selection.clone(), text: inner.into(), select: selection.start..selection.start + inner.len() };
    }
    Edit { range: selection.clone(), text: format!("{mark}{picked}{mark}"), select: selection.start + n..selection.end + n }
}

/// Makes the selection a link, the cursor left where its other half goes: the address, or the words for a selected one.
pub fn link(text: &str, selection: Range<usize>) -> Edit {
    let picked = &text[selection.clone()];
    let (text, at) = if is_url(picked) { (format!("[]({picked})"), 1) } else { (format!("[{picked}]()"), picked.len() + 3) };
    let at = selection.start + at;
    Edit { range: selection, text, select: at..at }
}

/// An address pasted over selected words links them to it.
pub fn pasted_link(picked: &str, pasted: &str) -> Option<String> {
    let url = pasted.trim();
    (is_url(url) && !picked.trim().is_empty() && !picked.contains('\n') && !is_url(picked.trim())).then(|| format!("[{picked}]({url})"))
}

/// Whether a pasted text is long enough to attach as a file.
pub fn long_paste(pasted: &str) -> bool {
    pasted.len() > LONG_PASTE_BYTES || pasted.lines().count() > LONG_PASTE_LINES
}

fn scheme(s: &str) -> Option<&'static str> {
    ["https://", "http://"].into_iter().find(|scheme| s.starts_with(scheme))
}

pub fn is_url(s: &str) -> bool {
    scheme(s).is_some_and(|scheme| s.len() > scheme.len() && !s.contains(char::is_whitespace))
}

/// `url` without the punctuation that ends a sentence or wraps it in emphasis; a `)` stays when it closes one the
/// address opened, as Wikipedia's do.
fn trimmed(url: &str) -> &str {
    let mut url = url;
    while let Some(last) = url.chars().next_back() {
        let opened = last == ')' && url.matches('(').count() >= url.matches(')').count();
        if opened || !matches!(last, '.' | ',' | ';' | ':' | '!' | '?' | ')' | '}' | '\'' | '*' | '_' | '~') {
            break;
        }
        url = &url[..url.len() - 1];
    }
    url
}

/// Each address in `line`.
fn urls(line: &str) -> Vec<Range<usize>> {
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(start) = line[from..].find("http").map(|i| from + i) {
        let rest = &line[start..];
        let before = line[..start].chars().next_back();
        let Some(scheme) = scheme(rest).filter(|_| before.is_none_or(|c| !c.is_alphanumeric())) else {
            from = start + 4;
            continue;
        };
        let end = rest.find(|c: char| c.is_whitespace() || matches!(c, '<' | '>' | '"' | '`' | '[' | ']')).unwrap_or(rest.len());
        let url = trimmed(&rest[..end]);
        if url.len() > scheme.len() {
            found.push(start..start + url.len());
        }
        from = start + end;
    }
    found
}

/// A line's addresses as links; one in `[words](address)` dims with the brackets, and its words are the link, an
/// address among them included.
fn links(line: &str, base: usize, out: &mut Vec<(Range<usize>, Style)>) {
    for url in urls(line) {
        if line[..url.start].ends_with('[') && line[url.end..].starts_with("](") {
            continue;
        }
        let label = line[..url.start].strip_suffix("](").and_then(|before| Some(before.rfind('[')? + 1..before.len()));
        let Some(label) = label.filter(|l| !l.is_empty()) else {
            out.push((base + url.start..base + url.end, Style::Link));
            continue;
        };
        let close = url.end + usize::from(line[url.end..].starts_with(')'));
        out.push((base + label.start - 1..base + label.start, Style::Syntax));
        out.push((base + label.start..base + label.end, Style::Link));
        out.push((base + label.end..base + close, Style::Syntax));
    }
}

/// Where `mark` closes what it opened before `from`: after a character that isn't a space, and not running on into
/// more of itself or, for `_`, a word.
fn closing(line: &str, from: usize, mark: &str) -> Option<usize> {
    line[from..].match_indices(mark).map(|(i, _)| from + i).find(|&at| {
        let after = line[at + mark.len()..].chars().next();
        let ends_word = line[..at].chars().next_back().is_some_and(|c| !c.is_whitespace() && !mark.starts_with(c));
        ends_word && at > from && !after.is_some_and(|c| mark.starts_with(c) || (mark == "_" && c.is_alphanumeric()))
    })
}

/// A line's emphasis: each mark dimmed around what it styles, and what it holds styled too. A mark opens before a
/// character that isn't a space, and `_` only at a word's start, so `snake_case` and `2 * 3` stay plain.
fn emphasis(line: &str, base: usize, out: &mut Vec<(Range<usize>, Style)>) {
    let mut at = 0;
    while let Some(c) = line[at..].chars().next() {
        let Some((mark, style)) = EMPHASIS.into_iter().find(|(mark, _)| line[at..].starts_with(mark)) else {
            at += c.len_utf8();
            continue;
        };
        let body = at + mark.len();
        let in_word = line[..at].chars().next_back().is_some_and(|c| mark.starts_with(c) || (mark == "_" && c.is_alphanumeric()));
        let opens = !in_word && line[body..].chars().next().is_some_and(|c| !c.is_whitespace() && !mark.starts_with(c));
        match opens.then(|| closing(line, body, mark)).flatten() {
            Some(close) => {
                out.push((base + at..base + body, Style::Syntax));
                out.push((base + body..base + close, style));
                out.push((base + close..base + close + mark.len(), Style::Syntax));
                emphasis(&line[body..close], base + body, out);
                at = close + mark.len();
            }
            None => at = body,
        }
    }
}

/// Where the link at `range` goes: the address its words are bracketed to, else itself.
pub fn link_target(text: &str, range: Range<usize>) -> &str {
    let words = &text[range.clone()];
    match text[range.end..].strip_prefix("](") {
        Some(after) => &after[..after.find([')', ' ', '\n']).unwrap_or(after.len())],
        _ => words,
    }
}

/// Each span of `text` to style, in order: its emphasis and links, a quote's `>`, and its fenced code. A list's
/// markers and inline code read as typed.
pub fn spans(text: &str) -> Vec<(Range<usize>, Style)> {
    let code = code(text);
    let mut out = Vec::new();
    let mut base = 0;
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches('\n');
        let mut from = 0;
        if let Some(item) = list_item(body) {
            let marker = item.indent.len()..item.indent.len() + item.width;
            from = marker.end + if item.check.is_some() { 4 } else { 0 };
            if item.marker == Marker::Quote {
                out.push((base + marker.start..base + marker.end, Style::Syntax));
            }
        }
        let from = from.min(body.len());
        let mut found = Vec::new();
        links(&body[from..], base + from, &mut found);
        let linked = found.clone();
        emphasis(&body[from..], base + from, &mut found);
        // An address keeps its `_` and `*`.
        found.retain(|(r, style)| *style == Style::Link || linked.iter().all(|(l, s)| *s != Style::Link || r.end <= l.start || l.end <= r.start || (l.start >= r.start && l.end <= r.end)));
        out.extend(found);
        base += line.len();
    }
    out.retain(|(r, _)| !in_code(&code, r));
    out.extend(code.iter().filter_map(|c| match c {
        Code::Block { open, close } if open.end < close.start => Some((open.end..close.start, Style::Code)),
        _ => None,
    }));
    out.sort_by_key(|(r, _)| (r.start, r.end));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `typed` with its `|`s taken out, and the selection they marked.
    fn at(typed: &str) -> (String, Range<usize>) {
        let start = typed.find('|').expect("a cursor");
        let text = typed.replacen('|', "", 1);
        let end = text[start..].find('|').map_or(start, |i| start + i);
        (text.replacen('|', "", 1), start..end)
    }

    /// The text an edit leaves, `|` at each end of its selection.
    fn after(typed: &str, edit: impl Fn(&str, Range<usize>) -> Option<Edit>) -> Option<String> {
        let (mut text, selection) = at(typed);
        let Edit { range, text: with, select } = edit(&text, selection)?;
        text.replace_range(range, &with);
        if !select.is_empty() {
            text.insert(select.end, '|');
        }
        text.insert(select.start, '|');
        Some(text)
    }

    fn entered(typed: &str) -> Option<String> {
        after(typed, newline)
    }

    fn styled(text: &str) -> Vec<(&str, Style)> {
        spans(text).into_iter().map(|(r, s)| (&text[r], s)).collect()
    }

    #[test]
    fn a_bullet_backed_into_or_undone_stays_where_it_is() {
        assert_eq!(list_started("- a", 2..2, 2..2), None, "a deletion types nothing");
        assert_eq!(list_started("- ", 0..2, 2..2).map(|e| e.text).as_deref(), Some("  "), "an input method's whole-text edit still ends in the space");
    }

    #[test]
    fn a_typed_range_the_text_no_longer_holds_starts_no_list() {
        assert_eq!(list_started("", 19..20, 0..0), None, "a send cleared the prompt before its edit was read");
    }

    #[test]
    fn a_new_line_in_a_list_opens_its_next_item() {
        assert_eq!(entered("- one|").as_deref(), Some("- one\n- |"));
        assert_eq!(entered("* one\n  + two|\nafter").as_deref(), Some("* one\n  + two\n  + |\nafter"));
        assert_eq!(entered("- one| two").as_deref(), Some("- one\n- | two"));
        assert_eq!(entered("> quoted|").as_deref(), Some("> quoted\n> |"));
    }

    #[test]
    fn a_new_line_in_an_empty_item_ends_the_list() {
        assert_eq!(entered("- one\n- |").as_deref(), Some("- one\n|"));
        assert_eq!(entered("1. one\n2. |\nafter").as_deref(), Some("1. one\n|\nafter"));
        assert_eq!(entered("- [ ] |").as_deref(), Some("|"));
        assert_eq!(entered("  - one\n  - |").as_deref(), Some("  - one\n|"), "a list that sits a step in is not nested");
    }

    #[test]
    fn a_list_started_by_its_space_at_a_lines_start_sits_a_step_in() {
        let typed = |typed: &str| after(typed, |text, selection| list_started(text, selection.start - 1..selection.start, selection));
        assert_eq!(typed("- |").as_deref(), Some("  - |"));
        assert_eq!(typed("intro\n1. |rest").as_deref(), Some("intro\n  1. |rest"));
        assert_eq!(typed("- one\n- |"), None, "under another item it is part of that list");
        assert_eq!(typed("  - one\n    - deep\n- |"), None);
        assert_eq!(typed("  - |"), None);
        assert_eq!(typed("- a|"), None, "past the marker");
        assert_eq!(typed("-|"), None);
        assert_eq!(typed("```\n- |"), None, "in a block being typed");
        assert_eq!(typed("> |"), None, "a quote stays at the margin");
    }

    #[test]
    fn a_new_line_outside_a_list_is_left_to_the_input() {
        assert_eq!(entered("plain|"), None);
        assert_eq!(entered("-not a list|"), None);
        assert_eq!(entered("|- before its marker"), None);
        assert_eq!(entered("- |picked| words"), None);
        assert_eq!(entered("```\n- in code|\n```"), None);
        assert_eq!(entered("```yaml\n- in a block being typed|"), None);
    }

    #[test]
    fn a_numbered_item_counts_on_and_those_after_it_follow() {
        assert_eq!(entered("1. one|").as_deref(), Some("1. one\n2. |"));
        assert_eq!(entered("9) nine|").as_deref(), Some("9) nine\n10) |"));
        assert_eq!(entered("1. one|\n2. two\n   its own line\n3. three\nafter").as_deref(), Some("1. one\n2. |\n3. two\n   its own line\n4. three\nafter"));
        assert_eq!(entered("1. one|\n2. two\n").as_deref(), Some("1. one\n2. |\n3. two\n"));
    }

    #[test]
    fn a_checklist_item_opens_the_next_one_unticked() {
        assert_eq!(entered("- [x] done|").as_deref(), Some("- [x] done\n- [ ] |"));
        assert_eq!(entered("- [link]|").as_deref(), Some("- [link]\n- |"), "words in brackets are no box");
    }

    #[test]
    fn tab_nests_a_list_item_and_shift_tab_brings_it_back() {
        let tab = |typed: &str, out: bool| after(typed, |text, selection| indent(text, selection, out));
        assert_eq!(tab("- one\n- tw|o", false).as_deref(), Some("- one\n  - tw|o"));
        assert_eq!(tab("- one\n  - tw|o", true).as_deref(), Some("- one\n- tw|o"));
        assert_eq!(tab("- one\n |- two", true).as_deref(), Some("- one\n|- two"));
        assert_eq!(tab("- one|", true), None, "nothing to bring back");
        assert_eq!(tab("  - one\n  - tw|o", true).as_deref(), Some("  - one\n- tw|o"), "back to the margin");
        assert_eq!(tab("```\n1. a\n2. b|\n```", false), None, "code stays as typed");
        assert_eq!(tab("- a\n- |b\n\ntail| text", false).as_deref(), Some("- a\n  - |b\n\ntail| text"), "a selection past the list keeps its end");
        assert_eq!(tab("plain|", false), None);
    }

    #[test]
    fn a_numbered_item_takes_its_number_from_where_tab_lands_it() {
        let tab = |typed: &str, out: bool| after(typed, |text, selection| indent(text, selection, out)).unwrap();
        assert_eq!(tab("1. one\n2. |", false), "1. one\n  1. |");
        assert_eq!(tab("1. one\n2. tw|o\n3. three\nafter", false), "1. one\n  1. tw|o\n2. three\nafter");
        assert_eq!(tab("1. one\n  1. a\n2. tw|o", false), "1. one\n  1. a\n  2. tw|o", "it follows the item already there");
        assert_eq!(tab("1. one\n  1. tw|o\n2. three", true), "1. one\n2. tw|o\n3. three");
        assert_eq!(tab("9. nine\n  1. te|n", true), "9. nine\n10. te|n");
        assert_eq!(tab("5. five\n- bullet\n6. s|ix", false), "5. five\n- bullet\n  1. s|ix", "a list keeps the number it starts at");
    }

    #[test]
    fn tab_moves_every_item_the_selection_is_on() {
        let tab = |typed: &str, out: bool| after(typed, |text, selection| indent(text, selection, out)).unwrap();
        assert_eq!(tab("- a\n- |b\n- c|", false), "- a\n  - |b\n  - c|");
        assert_eq!(tab("1. a\n2. |b\n3. c|\n4. d", false), "1. a\n  1. |b\n  2. c|\n2. d");
        assert_eq!(tab("- a\n  - |b\n  - c|", true), "- a\n- |b\n- c|");
    }

    #[test]
    fn a_new_line_in_an_empty_nested_item_ends_only_its_depth() {
        assert_eq!(entered("1. one\n  1. a\n  2. |\n2. two").as_deref(), Some("1. one\n  1. a\n2. |\n3. two"));
        assert_eq!(entered("- one\n  - |").as_deref(), Some("- one\n- |"));
    }

    #[test]
    fn a_format_key_wraps_the_selection_and_unwraps_it_again() {
        let bold = |typed: &str| after(typed, |text, selection| Some(toggle(text, selection, "**"))).unwrap();
        assert_eq!(bold("make |this| bold"), "make **|this|** bold");
        assert_eq!(bold("make **|this|** bold"), "make |this| bold");
        assert_eq!(bold("make |**this**| bold"), "make |this| bold");
    }

    #[test]
    fn a_link_leaves_the_cursor_where_its_other_half_goes() {
        let linked = |typed: &str| after(typed, |text, selection| Some(link(text, selection))).unwrap();
        assert_eq!(linked("see |the docs| now"), "see [the docs](|) now");
        assert_eq!(linked("see |https://a.dev/x|"), "see [|](https://a.dev/x)");
    }

    #[test]
    fn an_address_pasted_over_words_links_them() {
        assert_eq!(pasted_link("the docs", " https://a.dev/x\n").as_deref(), Some("[the docs](https://a.dev/x)"));
        assert_eq!(pasted_link("", "https://a.dev"), None);
        assert_eq!(pasted_link("the docs", "not an address"), None);
        assert_eq!(pasted_link("https://old.dev", "https://a.dev"), None, "an address is replaced");
        assert_eq!(pasted_link("two\nlines", "https://a.dev"), None);
    }

    #[test]
    fn a_paste_past_eighty_lines_or_eight_thousand_bytes_is_long() {
        assert!(!long_paste(&"line\n".repeat(80)));
        assert!(long_paste(&"line\n".repeat(81)));
        assert!(long_paste(&"x".repeat(8001)));
    }

    #[test]
    fn addresses_are_links_without_the_punctuation_around_them() {
        assert_eq!(styled("see https://a.dev/x_y, (http://b.dev/p?q=1). xhttp://no and https://"), [("https://a.dev/x_y", Style::Link), ("http://b.dev/p?q=1", Style::Link)]);
    }

    #[test]
    fn a_markdown_links_words_are_the_link_and_its_address_dims() {
        assert_eq!(styled("read [the docs](https://a.dev) first"), [("[", Style::Syntax), ("the docs", Style::Link), ("](https://a.dev)", Style::Syntax)]);
    }

    #[test]
    fn a_huge_number_counts_on_without_overflowing() {
        assert_eq!(entered("18446744073709551615. x|").as_deref(), Some("18446744073709551615. x\n18446744073709551615. |"));
    }

    #[test]
    fn a_link_leaves_out_the_marks_and_brackets_around_it() {
        let linked = |text: &'static str| -> Vec<&'static str> { spans(text).into_iter().filter(|(_, s)| *s == Style::Link).map(|(r, _)| link_target(text, r)).collect() };
        assert_eq!(linked("**https://a.dev** and _https://b.dev_"), ["https://a.dev", "https://b.dev"]);
        assert_eq!(linked("[https://a.dev](https://b.dev)"), ["https://b.dev"]);
        assert_eq!(linked("see https://en.wikipedia.org/wiki/Rust_(programming_language)."), ["https://en.wikipedia.org/wiki/Rust_(programming_language)"]);
    }

    #[test]
    fn emphasis_inside_emphasis_is_styled_too() {
        assert_eq!(styled("**_x_**"), [("**", Style::Syntax), ("_", Style::Syntax), ("_x_", Style::Bold), ("x", Style::Italic), ("_", Style::Syntax), ("**", Style::Syntax)]);
    }

    #[test]
    fn list_edits_land_on_whole_characters_in_vietnamese_text() {
        let text = "Kế hoạch:\n1. sửa lỗi **đường dẫn**\n  - kiểm tra _tiếng Việt_\n- [ ] gửi https://ví-dụ.vn/trang";
        for at in (0..=text.len()).filter(|&at| text.is_char_boundary(at)) {
            for edit in [newline(text, at..at), indent(text, at..at, false), indent(text, at..at, true)].into_iter().flatten() {
                let mut out = text.to_string();
                out.replace_range(edit.range, &edit.text);
                assert!(out.is_char_boundary(edit.select.start) && out.is_char_boundary(edit.select.end), "{at}");
            }
        }
    }

    #[test]
    fn a_link_goes_to_its_own_address_or_the_one_its_words_are_bracketed_to() {
        let text = "read [the docs](https://a.dev/x) or https://b.dev";
        let targets: Vec<&str> = spans(text).into_iter().filter(|(_, s)| *s == Style::Link).map(|(r, _)| link_target(text, r)).collect();
        assert_eq!(targets, ["https://a.dev/x", "https://b.dev"]);
    }

    #[test]
    fn emphasis_styles_what_its_marks_hold_and_dims_the_marks() {
        assert_eq!(styled("**bold** and _it_"), [("**", Style::Syntax), ("bold", Style::Bold), ("**", Style::Syntax), ("_", Style::Syntax), ("it", Style::Italic), ("_", Style::Syntax)]);
        assert_eq!(styled("~~gone~~ *it*").iter().map(|(_, s)| *s).filter(|s| *s != Style::Syntax).collect::<Vec<_>>(), [Style::Strike, Style::Italic]);
    }

    #[test]
    fn names_sums_and_open_marks_stay_plain() {
        for plain in ["snake_case_name", "2 * 3 * 4", "**open", "a ** b ** c", "__init__.py", "****"] {
            assert_eq!(styled(plain), [], "{plain:?}");
        }
    }

    #[test]
    fn a_lists_markers_read_as_typed_and_a_quotes_dims() {
        assert_eq!(styled("- one\n  12. two\n- [x] three\n> four"), [(">", Style::Syntax)]);
        assert_eq!(styled("* **bold** item"), [("**", Style::Syntax), ("bold", Style::Bold), ("**", Style::Syntax)]);
    }

    #[test]
    fn code_reads_as_typed_and_a_fenced_blocks_body_is_code() {
        assert_eq!(styled("`**a**` and\n```\n- **b**\n```"), [("\n- **b**\n", Style::Code)]);
    }
}
