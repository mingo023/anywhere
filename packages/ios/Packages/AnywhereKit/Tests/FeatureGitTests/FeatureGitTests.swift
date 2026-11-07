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

@Test func the_branch_card_measures_against_the_base_in_words() {
    var s = Sample.gitStatus(files: 1)
    #expect(GitText.branchLine(GitHead(s)) == "calm-otter → main")
    #expect(GitText.distance(GitHead(s)) == "4 ahead · 1 behind main")
    #expect(GitText.sinceBase(GitHead(s)) == "4 commits since main")
    s.base = nil
    #expect(GitText.branchLine(GitHead(s)) == "calm-otter")
    #expect(GitText.distance(GitHead(s)) == nil)
}

@Test func pull_counts_what_the_upstream_has_and_hides_when_there_is_nothing() {
    var s = Sample.gitStatus(files: 1)
    #expect(GitText.pull(GitHead(s)) == "Pull")
    s.unpulled = 12
    #expect(GitText.pull(GitHead(s)) == "Pull 12")
    s.unpulled = 0
    #expect(GitText.pull(GitHead(s)) == nil)
    s.upstream = nil
    #expect(GitText.pull(GitHead(s)) == nil)
}

@Test func commit_all_follows_the_mac_and_defaults_on_for_an_older_pocketd() {
    var s = Sample.gitStatus(files: 1)
    #expect(GitHead(s).commitAll)
    s.commitAll = false
    #expect(!GitHead(s).commitAll)
}

@Test func stage_all_carries_its_count_and_says_so_while_it_runs() {
    #expect(GitText.stageAll(.changes, count: 12, bulk: nil) == "Stage All 12")
    #expect(GitText.stageAll(.staged, count: 3, bulk: nil) == "Unstage All 3")
    let bulk = BulkStage(section: .changes, count: 1200)
    #expect(GitText.stageAll(.changes, count: 0, bulk: bulk) == "Staging \(1200.formatted())…")
    #expect(GitText.stageAll(.staged, count: 3, bulk: bulk) == "Unstage All 3")
}

@Test func the_discard_confirm_says_untracked_files_get_deleted() {
    let files = [
        GitFile(path: "a.swift", status: .modified, staged: false, unstaged: true),
        GitFile(path: "b.swift", status: .untracked, staged: false, unstaged: true),
        GitFile(path: "c.swift", status: .untracked, staged: false, unstaged: true),
    ]
    #expect(GitText.discardFacts(["a.swift"], files: files) == "Unstaged edits can't be restored")
    #expect(GitText.discardFacts(["a.swift", "b.swift"], files: files) == "Unstaged edits can't be restored\nDeletes 1 untracked file")
    #expect(GitText.discardFacts(["b.swift", "c.swift"], files: files) == "Unstaged edits can't be restored\nDeletes 2 untracked files")
}

@Test func the_change_bar_counts_changes_then_shows_where_it_is() {
    #expect(GitText.position(nil, of: 42) == "42 changes")
    #expect(GitText.position(nil, of: 1) == "1 change")
    #expect(GitText.position(2, of: 42) == "3 of 42")
}

@Test func stepping_files_stops_at_the_ends_and_a_staged_file_gives_way_to_the_next() {
    let files = ["a", "b", "c"]
    #expect(DiffFiles.step(from: "b", by: 1, in: files) == "c")
    #expect(DiffFiles.step(from: "c", by: 1, in: files) == nil)
    #expect(DiffFiles.step(from: "x", by: 1, in: files) == nil)
    #expect(DiffFiles.after(removing: "b", in: files) == "c")
    #expect(DiffFiles.after(removing: "c", in: files) == "b")
    #expect(DiffFiles.after(removing: "a", in: ["a"]) == nil)
}

@Test func stepping_changes_starts_at_either_end_and_stops_at_the_ends() {
    #expect(ChangeStep.step(from: nil, by: 1, count: 3) == 0)
    #expect(ChangeStep.step(from: nil, by: -1, count: 3) == 2)
    #expect(ChangeStep.step(from: 2, by: 1, count: 3) == nil)
    #expect(ChangeStep.step(from: 0, by: -1, count: 3) == nil)
    #expect(ChangeStep.step(from: nil, by: 1, count: 0) == nil)
}

@Test func a_reloaded_diff_with_fewer_changes_keeps_the_stepped_change_within_them() {
    #expect(ChangeStep.kept(5, count: 3) == 2)
    #expect(ChangeStep.kept(1, count: 3) == 1)
    #expect(ChangeStep.kept(1, count: 0) == nil)
    #expect(ChangeStep.kept(nil, count: 3) == nil)
    #expect(ChangeStep.step(from: ChangeStep.kept(5, count: 3), by: -1, count: 3) == 1)
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
