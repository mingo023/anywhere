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

@Test func sections_cut_flat_rows_at_their_headers() {
    let files = [
        GitFile(path: "a.swift", status: .modified, staged: true, unstaged: true),
        GitFile(path: "b.swift", status: .untracked, staged: false, unstaged: true),
    ]
    let sections = SectionRows.split(ChangeTree.rows(files, tree: false, folded: []))
    #expect(sections.map(\.section) == [.staged, .changes])
    #expect(sections[0].paths == ["a.swift"])
    #expect(sections[1].rows.count == 2)
}

@Test func a_selection_picks_files_of_its_own_section_only() {
    let selection: Set = ["staged:a.swift", "changes:b.swift", "changes:dir/", "changes:dir/c.swift"]
    #expect(SectionRows.paths(selection, in: .changes) == ["b.swift", "dir/c.swift"])
    #expect(SectionRows.paths(selection, in: .staged) == ["a.swift"])
}

@Test func tips_label_the_commits_they_point_at_by_name() {
    let tips = LogTips(head: "h", branch: "calm-otter", upstream: "u", base: "h")
    let head = GitHead(Sample.gitStatus(files: 1))
    #expect(HistoryScreen.labels("h", tips: tips, head: head).map(\.name) == ["calm-otter", "main"])
    #expect(HistoryScreen.labels("u", tips: tips, head: head).map(\.name) == ["origin/calm-otter"])
    #expect(HistoryScreen.labels("x", tips: tips, head: head).isEmpty)
}
