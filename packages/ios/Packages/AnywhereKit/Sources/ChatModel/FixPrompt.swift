import Wire

/// What an agent is sent to fix a PR, worded as the desktop's `fix_prompt` and `conflict_prompt` (pull_request.rs).
public enum FixPrompt {
    /// Each failed check with its log, each review thread with the code it's on, numbered.
    public static func fix(_ pr: PullRequest, checks: [CheckRun], threads: [ReviewThread]) -> String {
        let failed = checks.map { c in "The check \"\(c.name)\" failed." + (c.url.map { " Its log: \($0)" } ?? "") }
        let commented = threads.map { t in
            let code = hunkTail(t.comments.first?.hunk).joined(separator: "\n")
            let said = t.comments.map { "\($0.author): \($0.body.trimmingCharacters(in: .whitespacesAndNewlines))" }
            return "A review comment on \(place(t)):\n```\n\(code)\n```\n" + said.joined(separator: "\n")
        }
        let items = (failed + commented).enumerated().map { "\($0.offset + 1). \($0.element)" }
        return "Fix what holds back PR #\(pr.number) (\(pr.url)):\n\n\(items.joined(separator: "\n\n"))\n\nThen commit and push."
    }

    /// Merging in the PR's `base`, named as gh does or as origin's copy.
    public static func conflict(base: String) -> String {
        let branch = base.hasPrefix("origin/") ? String(base.dropFirst("origin/".count)) : base
        return "Merge origin/\(branch) into this branch and resolve the conflicts, then push."
    }

    /// The last `n` lines of a comment's hunk, which ends at the line it's on (`hunk_tail`).
    public static func hunkTail(_ hunk: String?, _ n: Int = 3) -> [String] {
        var lines = (hunk ?? "").split(separator: "\n", omittingEmptySubsequences: false)
        if lines.last == "" { lines.removeLast() }
        return lines.filter { !$0.hasPrefix("@@") }.suffix(n).map(String.init)
    }

    /// `path:line`, or the path alone for a thread on the whole file (`place`).
    public static func place(_ t: ReviewThread) -> String {
        t.line.map { "\(t.path):\($0)" } ?? t.path
    }
}
