use crate::{Blobs, MAX_READ, Runner, git_bytes, lossy};
use imara_diff::{Algorithm, Diff, InternedInput};
use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;

pub fn file_diff(run: &Runner, cwd: &str, path: &str) -> Vec<Line> {
    let (old, new) = texts(run, cwd, path);
    diff_texts(&old, &new, &HashSet::new(), Options::default())
}

/// How a diff is drawn: unchanged lines kept around each change, and whether changes only in whitespace count.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Options {
    pub context: usize,
    pub ignore_whitespace: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { context: 3, ignore_whitespace: false }
    }
}

/// The file at HEAD (empty when untracked) and in the working tree (empty when deleted); `path` is relative to `cwd` or absolute.
pub fn texts(run: &Runner, cwd: &str, path: &str) -> (String, String) {
    lossy(blobs(run, cwd, path))
}

pub fn blobs(run: &Runner, cwd: &str, path: &str) -> Blobs {
    let full = Path::new(cwd).join(path);
    let rel = full.strip_prefix(cwd).unwrap_or(&full).to_string_lossy().into_owned();
    let old = git_bytes(run, cwd, &["show", &format!("HEAD:./{rel}")]).unwrap_or_default();
    let new = run.read(&full.to_string_lossy(), MAX_READ).unwrap_or_default();
    (old, new)
}

/// A unified diff of two texts with `options.context` lines of context around each change. A gap whose first new-side line is in `open` shows in full instead of folding.
pub fn diff_texts(old: &str, new: &str, open: &HashSet<usize>, options: Options) -> Vec<Line> {
    if old.contains('\0') || new.contains('\0') {
        return Vec::new();
    }
    let (ol, nl): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    // With whitespace ignored, lines are compared without it but shown as written.
    let key = |l: &str| if options.ignore_whitespace { l.split_whitespace().collect() } else { l.to_string() };
    let mut input = InternedInput::default();
    input.update_before(imara_diff::sources::lines(old).map(key));
    input.update_after(imara_diff::sources::lines(new).map(key));
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let hunks: Vec<_> = diff.hunks().collect();
    if hunks.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut o, mut n) = (0, 0);
    for k in 0..=hunks.len() {
        let last = k == hunks.len();
        let (bs, be, as_, ae) = hunks.get(k).map_or((ol.len(), ol.len(), nl.len(), nl.len()), |h| {
            (h.before.start as usize, h.before.end as usize, h.after.start as usize, h.after.end as usize)
        });
        let gap = bs - o;
        let lead = if k == 0 { 0 } else { gap.min(options.context) };
        let trail = if last { 0 } else { (gap - lead).min(options.context) };
        let fold = if last || open.contains(&(n + lead + 1)) { 0 } else { gap - lead - trail };
        let same = |i: usize| Line { kind: Kind::Context, old: Some(o + i + 1), new: Some(n + i + 1), text: nl[n + i].into() };
        out.extend((0..lead).map(same));
        if k == 0 || fold > 0 {
            let (a, b) = (o + lead + fold + 1, n + lead + fold + 1);
            out.push(Line { kind: Kind::Hunk, old: None, new: None, text: format!("@@ -{a} +{b} @@") });
        }
        out.extend((lead + fold..if last { lead } else { gap }).map(same));
        out.extend((bs..be).map(|i| Line { kind: Kind::Del, old: Some(i + 1), new: None, text: ol[i].into() }));
        out.extend((as_..ae).map(|i| Line { kind: Kind::Add, old: None, new: Some(i + 1), text: nl[i].into() }));
        (o, n) = (be, ae);
    }
    out
}

fn tokens(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            start.get_or_insert(i);
            continue;
        }
        if let Some(a) = start.take() {
            out.push((a, &s[a..i]));
        }
        out.push((i, &s[i..i + c.len_utf8()]));
    }
    if let Some(a) = start {
        out.push((a, &s[a..]));
    }
    out
}

/// Byte ranges that differ between two versions of a line, compared word by word: (removed from `old`, added in `new`).
pub fn words(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    let mut input = InternedInput::default();
    input.update_before(a.iter().map(|t| t.1));
    input.update_after(b.iter().map(|t| t.1));
    let span = |t: &[(usize, &str)], r: Range<u32>| t[r.start as usize].0..t[r.end as usize - 1].0 + t[r.end as usize - 1].1.len();
    let (mut del, mut add) = (Vec::new(), Vec::new());
    for h in Diff::compute(Algorithm::Histogram, &input).hunks() {
        if !h.before.is_empty() {
            del.push(span(&a, h.before));
        }
        if !h.after.is_empty() {
            add.push(span(&b, h.after));
        }
    }
    (del, add)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Hunk,
    Context,
    Add,
    Del,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: String,
}

pub fn hunk_start(header: &str, sign: char) -> usize {
    header.split_whitespace().find_map(|w| w.strip_prefix(sign)).and_then(|w| w.split(',').next()?.parse().ok()).unwrap_or(1)
}

pub fn parse(diff: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0, 0);
    let mut body = false;
    for l in diff.lines() {
        if l.starts_with("@@") {
            body = true;
            old = hunk_start(l, '-');
            new = hunk_start(l, '+');
            out.push(Line { kind: Kind::Hunk, old: None, new: None, text: l.into() });
            continue;
        }
        if !body {
            continue;
        }
        let (kind, text) = match l.split_at_checked(1) {
            Some(("+", t)) => (Kind::Add, t),
            Some(("-", t)) => (Kind::Del, t),
            Some((" ", t)) => (Kind::Context, t),
            None => (Kind::Context, ""),
            _ => continue,
        };
        let line = Line { kind, old: (kind != Kind::Add).then_some(old), new: (kind != Kind::Del).then_some(new), text: text.into() };
        old += (kind != Kind::Add) as usize;
        new += (kind != Kind::Del) as usize;
        out.push(line);
    }
    out
}

/// Pairs each run of deletions with the additions that follow it, side by side.
pub fn split(lines: &[Line]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if matches!(lines[i].kind, Kind::Hunk | Kind::Context) {
            out.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let dels = lines[i..].iter().take_while(|l| l.kind == Kind::Del).count();
        let adds = lines[i + dels..].iter().take_while(|l| l.kind == Kind::Add).count();
        for k in 0..dels.max(adds) {
            out.push(((k < dels).then_some(i + k), (k < adds).then_some(i + dels + k)));
        }
        i += dels + adds;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    const DIFF: &str = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -45,4 +45,5 @@ fn a() {\n ctx\n-old\n+new1\n+new2\n ctx2\n\\ No newline at end of file\n";

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("l{i}\n")).collect()
    }

    #[test]
    fn parses_hunks_with_line_numbers() {
        let l = parse(DIFF);
        let got: Vec<_> = l.iter().map(|l| (l.kind, l.old, l.new, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Hunk, None, None, "@@ -45,4 +45,5 @@ fn a() {"),
                (Kind::Context, Some(45), Some(45), "ctx"),
                (Kind::Del, Some(46), None, "old"),
                (Kind::Add, None, Some(46), "new1"),
                (Kind::Add, None, Some(47), "new2"),
                (Kind::Context, Some(47), Some(48), "ctx2"),
            ]
        );
    }

    #[test]
    fn split_pairs_deletions_with_additions() {
        let l = parse(DIFF);
        let text = |i: Option<usize>| i.map(|i| l[i].text.as_str());
        let rows: Vec<_> = split(&l).into_iter().map(|(a, b)| (text(a), text(b))).collect();
        assert_eq!(rows[2], (Some("old"), Some("new1")));
        assert_eq!(rows[3], (None, Some("new2")));
        assert_eq!(rows.len(), 5);
    }

    #[test]
    #[allow(clippy::single_range_in_vec_init)]
    fn diffs_word_by_word() {
        assert_eq!(words("let a = 1;", "let b = 1;"), (vec![4..5], vec![4..5]));
        assert_eq!(words("foo(bar)", "foo(bar, baz)"), (vec![], vec![7..12]));
        assert_eq!(words("é = 1", "é = 2"), (vec![5..6], vec![5..6]));
    }

    #[test]
    fn reads_head_and_working_texts() {
        let dir = std::env::temp_dir().join(format!("pocket-git-texts-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let d = dir.to_str().unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(d).args(args).output().unwrap().status.success());
        run(&["init", "-q"]);
        std::fs::write(dir.join("src/a.rs"), "old\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
        std::fs::write(dir.join("src/a.rs"), "new\n").unwrap();
        std::fs::write(dir.join("b.rs"), "fresh\n").unwrap();
        assert_eq!(texts(&Runner::Local, d, "src/a.rs"), ("old\n".into(), "new\n".into()));
        assert_eq!(texts(&Runner::Local, &format!("{d}/src"), &format!("{d}/src/a.rs")), ("old\n".into(), "new\n".into()));
        assert_eq!(texts(&Runner::Local, d, "b.rs"), (String::new(), "fresh\n".into()));
        std::fs::write(dir.join("src/a.rs"), b"\x89PNG\0\xff").unwrap();
        assert!(file_diff(&Runner::Local, d, "src/a.rs").is_empty());
        assert_eq!(blobs(&Runner::Local, d, "src/a.rs"), (b"old\n".to_vec(), b"\x89PNG\0\xff".to_vec()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn diffs_texts_with_three_lines_of_context() {
        let old = numbered(20);
        let lines = diff_texts(&old, &old.replace("l10\n", "L10\n"), &HashSet::new(), Options::default());
        let got: Vec<_> = lines.iter().map(|l| (l.kind, l.old, l.new, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Hunk, None, None, "@@ -7 +7 @@"),
                (Kind::Context, Some(7), Some(7), "l7"),
                (Kind::Context, Some(8), Some(8), "l8"),
                (Kind::Context, Some(9), Some(9), "l9"),
                (Kind::Del, Some(10), None, "l10"),
                (Kind::Add, None, Some(10), "L10"),
                (Kind::Context, Some(11), Some(11), "l11"),
                (Kind::Context, Some(12), Some(12), "l12"),
                (Kind::Context, Some(13), Some(13), "l13"),
            ]
        );
    }

    #[test]
    fn folds_long_gaps_and_opens_them_on_request() {
        let old = numbered(30);
        let new = old.replace("l5\n", "L5\n").replace("l25\n", "L25\n");
        let folded = diff_texts(&old, &new, &HashSet::new(), Options::default());
        let headers: Vec<&str> = folded.iter().filter(|l| l.kind == Kind::Hunk).map(|l| l.text.as_str()).collect();
        assert_eq!(headers, ["@@ -2 +2 @@", "@@ -22 +22 @@"]);
        let open = diff_texts(&old, &new, &HashSet::from([9]), Options::default());
        assert_eq!(open.iter().filter(|l| l.kind == Kind::Hunk).count(), 1);
        assert_eq!(open.len(), folded.len() - 1 + 13);
    }

    #[test]
    fn merges_nearby_changes_and_handles_edges() {
        let old = numbered(10);
        let near = diff_texts(&old, &old.replace("l1\n", "L1\n").replace("l6\n", "L6\n"), &HashSet::new(), Options::default());
        assert_eq!(near[0].text, "@@ -1 +1 @@");
        assert_eq!(near.iter().filter(|l| l.kind == Kind::Hunk).count(), 1);
        assert!(diff_texts(&old, &old, &HashSet::new(), Options::default()).is_empty());
        assert!(diff_texts("a\0", "b", &HashSet::new(), Options::default()).is_empty());
        let added = diff_texts("", "x\ny\n", &HashSet::new(), Options::default());
        assert_eq!(added.iter().map(|l| l.kind).collect::<Vec<_>>(), [Kind::Hunk, Kind::Add, Kind::Add]);
    }

    #[test]
    fn context_lines_widen_or_narrow_each_hunk() {
        let old = numbered(20);
        let new = old.replace("l10\n", "L10\n");
        let around =
            |context| diff_texts(&old, &new, &HashSet::new(), Options { context, ..Options::default() }).iter().filter(|l| l.kind == Kind::Context).count();
        assert_eq!((around(0), around(3), around(5)), (0, 6, 10));
    }

    #[test]
    fn ignoring_whitespace_hides_reindented_lines_but_shows_them_as_written() {
        let old = "fn a() {\n    x();\n}\n";
        let new = "fn a() {\n        x();\n}\ny();\n";
        let ignore = Options { ignore_whitespace: true, ..Options::default() };
        let kinds = |o| diff_texts(old, new, &HashSet::new(), o).into_iter().filter(|l| l.kind != Kind::Hunk).map(|l| (l.kind, l.text)).collect::<Vec<_>>();
        assert!(kinds(Options::default()).contains(&(Kind::Del, "    x();".into())));
        let ignored = kinds(ignore);
        assert!(ignored.contains(&(Kind::Context, "        x();".into())));
        assert_eq!(ignored.iter().filter(|(k, _)| *k != Kind::Context).collect::<Vec<_>>(), [&(Kind::Add, "y();".to_string())]);
    }
}
