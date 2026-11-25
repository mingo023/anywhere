//! The first prompt of a new session on the host, when the conversation itself can't move.

/// What this Mac knows of the session, for its summary.
pub struct Known<'a> {
    pub title: &'a str,
    pub branch: &'a str,
    pub files: usize,
    /// The user's last request, if the timeline has one.
    pub asked: Option<&'a str>,
    /// The agent's last reply, if any.
    pub replied: Option<&'a str>,
}

const MAX_REPLY: usize = 1200;

/// A summary the user reviews and edits before it's sent; empty when this Mac knows nothing of the conversation.
pub fn draft(k: &Known) -> String {
    let (Some(asked), replied) = (k.asked, k.replied) else { return String::new() };
    let mut out = format!("We're continuing “{}” on branch {}", k.title, k.branch);
    out += &match k.files {
        0 => ".".to_string(),
        1 => ", with 1 changed file.".to_string(),
        n => format!(", with {n} changed files."),
    };
    out += &format!("\n\nLast request:\n{}", asked.trim());
    if let Some(r) = replied.map(str::trim).filter(|r| !r.is_empty()) {
        let cut: String = r.chars().take(MAX_REPLY).collect();
        let more = if cut.len() < r.len() { "…" } else { "" };
        out += &format!("\n\nWhere it got to:\n{cut}{more}");
    }
    out + "\n\nPick up from here."
}

#[cfg(test)]
mod tests {
    use super::{Known, draft};

    #[test]
    fn a_summary_names_the_branch_the_last_request_and_where_it_got_to() {
        let k = Known { title: "Fix stale reveal", branch: "fix/reveal", files: 3, asked: Some("Fix it"), replied: Some("Moved the reveal.") };
        assert_eq!(
            draft(&k),
            "We're continuing “Fix stale reveal” on branch fix/reveal, with 3 changed files.\n\nLast request:\nFix it\n\nWhere it got to:\nMoved the reveal.\n\nPick up from here."
        );
    }

    #[test]
    fn nothing_known_of_the_conversation_leaves_it_for_the_user_to_write() {
        assert_eq!(draft(&Known { title: "t", branch: "b", files: 0, asked: None, replied: None }), "");
    }

    #[test]
    fn a_long_reply_is_cut() {
        let long = "x".repeat(5000);
        let k = Known { title: "t", branch: "b", files: 1, asked: Some("go"), replied: Some(&long) };
        let d = draft(&k);
        assert!(d.contains("with 1 changed file."));
        assert!(d.len() < 1500 && d.contains("x…"));
    }
}
