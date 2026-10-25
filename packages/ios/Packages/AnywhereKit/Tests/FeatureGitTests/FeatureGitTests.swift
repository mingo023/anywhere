import DiffModel
import Fixtures
import Stores
import Testing
import Wire

@testable import FeatureGit

@Test func ages_read_as_minutes_hours_days_then_a_date() {
    let now = 1_792_832_400
    #expect(GitText.age(now - 30, now: now) == "now")
    #expect(GitText.age(now - 300, now: now) == "5m")
    #expect(GitText.age(now - 3 * 3_600, now: now) == "3h")
    #expect(GitText.age(now - 2 * 86_400, now: now) == "2d")
    #expect(GitText.age(0, now: now) == "1 Jan 1970")
}

@Test func push_counts_unpushed_commits_and_publishes_a_branch_without_upstream() {
    var s = Sample.gitStatus(files: 3)
    #expect(GitText.push(GitHead(s)) == "Push 2")
    s.unpushed = 0
    #expect(GitText.push(GitHead(s)) == nil)
    s.unpushed = nil
    #expect(GitText.push(GitHead(s)) == "Publish Branch")
    s.branch = nil
    s.detached = true
    #expect(GitText.push(GitHead(s)) == nil)
}

@Test func ahead_and_behind_show_only_what_differs() {
    var s = Sample.gitStatus(files: 1)
    #expect(GitText.aheadBehind(GitHead(s)) == "↑4 ↓1")
    s.behind = 0
    #expect(GitText.aheadBehind(GitHead(s)) == "↑4")
    s.ahead = 0
    #expect(GitText.aheadBehind(GitHead(s)) == nil)
}

@Test func a_path_splits_into_name_and_folder() {
    #expect(GitText.split("a/b/c.swift") == ("c.swift", "a/b"))
    #expect(GitText.split("README.md") == ("README.md", ""))
}

@Test func each_section_s_rows_make_one_card_under_its_header() {
    let files = [
        GitFile(path: "a.swift", status: .modified, staged: true, unstaged: true),
        GitFile(path: "b.swift", status: .untracked, staged: false, unstaged: true),
    ]
    let lines = ChangeLine.lines(ChangeTree.rows(files, tree: false, folded: []))
    #expect(lines.map(\.id) == ["staged", "staged:a.swift", "changes", "changes:a.swift", "changes:b.swift"])
    #expect(lines.map(\.first) == [false, true, false, true, false])
    #expect(lines.map(\.last) == [false, true, false, false, true])
}

@Test func a_selection_picks_files_of_its_own_section_only() {
    let selection: Set = ["staged:a.swift", "changes:b.swift", "changes:dir/", "changes:dir/c.swift"]
    #expect(ChangeLine.paths(selection, in: .changes) == ["b.swift", "dir/c.swift"])
    #expect(ChangeLine.paths(selection, in: .staged) == ["a.swift"])
}

@MainActor
@Test func the_list_keeps_its_lines_until_the_files_or_folds_change() {
    let host = HostModel.offline(hello: Sample.hello, messages: [])
    host.git.take(Sample.gitStatus(files: 3))
    let changes = host.git.changes(Sample.tree)
    let cache = ChangeLines()
    let first = cache.lines(changes, tree: false, folded: [])
    #expect(cache.lines(changes, tree: false, folded: []) == first)
    #expect(cache.lines(changes, tree: false, folded: ["changes"]).count < first.count)
    var next = Sample.gitStatus(files: 3)
    next.files[0].added = 99
    host.git.take(next)
    let moved = cache.lines(changes, tree: false, folded: [])
    #expect(moved != first)
    #expect(moved.map(\.id) == first.map(\.id))
}

@Test func tips_label_the_commits_they_point_at_by_name() {
    let tips = LogTips(head: "h", branch: "calm-otter", upstream: "u", base: "h")
    let head = GitHead(Sample.gitStatus(files: 1))
    #expect(HistoryScreen.labels("h", tips: tips, head: head).map(\.name) == ["calm-otter", "main"])
    #expect(HistoryScreen.labels("u", tips: tips, head: head).map(\.name) == ["origin/calm-otter"])
    #expect(HistoryScreen.labels("x", tips: tips, head: head).isEmpty)
}
