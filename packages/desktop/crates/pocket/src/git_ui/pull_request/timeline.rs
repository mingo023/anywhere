use git::github::{Pr, Thread};

/// One card of the Conversation; indexes point into the PR's lists.
#[derive(Debug, PartialEq)]
pub(crate) enum Entry {
    /// A review, with the threads it opened that are shown.
    Review { at: usize, threads: Vec<usize> },
    Note(usize),
    /// A thread whose review isn't among those fetched.
    Thread(usize),
}

fn opened_in(t: &Thread) -> Option<&str> {
    t.comments.first()?.review.as_deref()
}

/// The Conversation, oldest first: each review with the threads it opened, then what was said on the PR itself, in time order.
/// Without `all`, resolved threads are left out, and a review left with neither a body nor a thread goes too, as a reply in a thread submits one.
pub(crate) fn timeline(pr: &Pr, all: bool) -> Vec<Entry> {
    let shown: Vec<usize> = (0..pr.threads.len()).filter(|&i| all || !pr.threads[i].resolved).collect();
    let mut entries: Vec<(u64, Entry)> = Vec::new();
    for (at, r) in pr.submitted.iter().enumerate() {
        let threads: Vec<usize> = shown.iter().copied().filter(|&i| opened_in(&pr.threads[i]) == Some(r.id.as_str())).collect();
        if !r.body.trim().is_empty() || !threads.is_empty() {
            entries.push((r.at, Entry::Review { at, threads }));
        }
    }
    entries.extend(pr.notes.iter().enumerate().map(|(i, n)| (n.at, Entry::Note(i))));
    for i in shown {
        let t = &pr.threads[i];
        if !opened_in(t).is_some_and(|id| pr.submitted.iter().any(|r| r.id == id)) {
            entries.push((t.comments.first().map_or(0, |c| c.at), Entry::Thread(i)));
        }
    }
    entries.sort_by_key(|(at, _)| *at);
    entries.into_iter().map(|(_, e)| e).collect()
}

/// What the Conversation tab counts: comments on the PR, review bodies, and every comment in a thread.
pub(crate) fn said(pr: &Pr) -> usize {
    pr.notes.len() + pr.submitted.iter().filter(|r| !r.body.trim().is_empty()).count() + pr.threads.iter().map(|t| t.comments.len()).sum::<usize>()
}

#[cfg(test)]
mod tests {
    use super::{Entry, said, timeline};
    use git::github::{Comment, Note, Pr, Submitted, Thread};

    fn thread(id: &str, review: Option<&str>, at: u64, resolved: bool) -> Thread {
        let c = Comment { review: review.map(str::to_string), at, ..Comment::default() };
        Thread { id: id.into(), resolved, comments: vec![c], ..Thread::default() }
    }

    fn review(id: &str, body: &str, at: u64) -> Submitted {
        Submitted { id: id.into(), body: body.into(), at, ..Submitted::default() }
    }

    fn pr() -> Pr {
        Pr {
            submitted: vec![review("R1", "One note", 10), review("R2", "", 30), review("R3", "", 40)],
            threads: vec![thread("T1", Some("R1"), 10, false), thread("T2", Some("R2"), 30, true), thread("T3", Some("gone"), 5, false)],
            notes: vec![Note { body: "Question".into(), at: 20, ..Note::default() }],
            ..Pr::default()
        }
    }

    #[test]
    fn reviews_and_comments_run_in_time_with_threads_under_the_review_that_opened_them() {
        assert_eq!(timeline(&pr(), true), [Entry::Thread(2), Entry::Review { at: 0, threads: vec![0] }, Entry::Note(0), Entry::Review { at: 1, threads: vec![1] }]);
    }

    #[test]
    fn unresolved_only_drops_resolved_threads_and_the_reviews_they_leave_empty() {
        assert_eq!(timeline(&pr(), false), [Entry::Thread(2), Entry::Review { at: 0, threads: vec![0] }, Entry::Note(0)]);
    }

    #[test]
    fn a_review_with_a_body_stays_when_its_threads_are_resolved() {
        let mut pr = pr();
        pr.threads[0].resolved = true;
        assert_eq!(timeline(&pr, false)[1], Entry::Review { at: 0, threads: vec![] });
    }

    #[test]
    fn the_tab_counts_comments_review_bodies_and_thread_comments() {
        assert_eq!(said(&pr()), 1 + 1 + 3);
        assert_eq!(said(&Pr::default()), 0);
    }
}
