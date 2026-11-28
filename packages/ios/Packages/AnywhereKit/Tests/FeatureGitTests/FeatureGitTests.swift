import DiffModel
import Fixtures
import Foundation
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

@Test func push_forces_only_over_a_rewritten_branch_and_only_when_the_mac_allows_it() {
    var s = Sample.gitStatus(files: 1)
    s.unpulled = 3
    #expect(GitHead(s).forceReplaces == nil)
    s.forceWithLease = true
    #expect(GitHead(s).forceReplaces == 3)
    s.unpulled = 0
    #expect(GitHead(s).forceReplaces == nil)
    s.unpulled = 3
    s.unpushed = 0
    #expect(GitHead(s).forceReplaces == nil)
    s.unpushed = nil
    s.unpulled = nil
    #expect(GitHead(s).forceReplaces == nil)
}

@Test func the_force_push_confirm_names_the_branch_what_it_replaces_and_the_remote() {
    var s = Sample.gitStatus(files: 1)
    s.forceWithLease = true
    #expect(GitText.forcePush(GitHead(s)) == nil)
    s.unpulled = 1
    #expect(GitText.forcePush(GitHead(s))! == ("Force push calm-otter?", "Replaces 1 commit on origin."))
    s.unpulled = 4
    s.upstream = "fork/team/calm-otter"
    #expect(GitText.forcePush(GitHead(s))! == ("Force push calm-otter?", "Replaces 4 commits on fork."))
}

@Test func stage_all_says_how_many_only_while_it_runs() {
    #expect(GitText.stageAll(.changes, bulk: nil) == "Stage All")
    #expect(GitText.stageAll(.staged, bulk: nil) == "Unstage All")
    let bulk = BulkStage(section: .changes, count: 1200)
    #expect(GitText.stageAll(.changes, bulk: bulk) == "Staging \(1200.formatted())…")
    #expect(GitText.stageAll(.staged, bulk: bulk) == "Unstage All")
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

@MainActor
@Test func a_filter_keeps_the_files_whose_path_matches_in_any_case() {
    let host = HostModel.offline(hello: Sample.hello, messages: [])
    host.git.take(Sample.gitStatus(files: 3))
    let changes = host.git.changes(Sample.tree)
    let path = changes.files[0].path
    let lines = ChangeLines().lines(changes, tree: false, folded: [], filter: " \(path.uppercased()) ")
    let files = lines.compactMap { line -> String? in
        if case .file(_, let f, _) = line.row { f.path } else { nil }
    }
    #expect(Set(files) == [path])
}

@Test func the_branch_row_shows_only_with_something_to_offer() {
    var s = Sample.gitStatus(files: 1)
    s.unpushed = 0
    s.unpulled = 0
    #expect(GitText.showsHead(GitHead(s)))
    s.ahead = 0
    #expect(!GitText.showsHead(GitHead(s)))
    s.detached = true
    #expect(GitText.showsHead(GitHead(s)))
    s.detached = false
    s.unpushed = 1
    #expect(GitText.showsHead(GitHead(s)))
}

@Test func the_diff_bar_says_which_file_of_how_many() {
    #expect(GitText.fileOf(1, of: 6) == "File 2 of 6")
    #expect(GitText.fileOf(nil, of: 1) == "1 file")
    #expect(GitText.fileOf(nil, of: 3) == "3 files")
}

@Test func a_long_path_drops_the_folders_between_its_first_and_its_file() {
    #expect(GitText.shortPath("src/checkout.ts") == "src/checkout.ts")
    #expect(GitText.shortPath("src/checkout/payments/retry/checkout.test.ts") == "src/…/checkout.test.ts")
    #expect(GitText.shortPath("packages/mobile/very-long-folder-name/a-quite-long-file-name.test.ts") == "…/a-quite-long-file-name.test.ts")
}

@Test func a_changed_file_wears_one_letter_for_its_status() {
    #expect(["A", "U", "D", "R", "C", "M"] == [FileStatus.added, .untracked, .deleted, .renamed, .conflict, .modified].map(GitText.letter))
}

@Test func a_compare_is_headed_by_the_commits_ahead_or_by_how_many_files_differ() {
    #expect(GitText.compareTitle(files: 4, ahead: 1, against: "main") == "1 commit ahead of main")
    #expect(GitText.compareTitle(files: 4, ahead: 5, against: "main") == "5 commits ahead of main")
    #expect(GitText.compareTitle(files: 1, ahead: nil, against: "main") == "1 file differ from main")
}

@Test func the_filter_shows_for_a_long_list_or_while_it_holds_a_filter() {
    #expect(!GitText.showsFilter(files: 11, filter: ""))
    #expect(!GitText.showsFilter(files: 3, filter: "  "))
    #expect(GitText.showsFilter(files: 12, filter: ""))
    #expect(GitText.showsFilter(files: 3, filter: "cart"))
}

@Test func the_offline_strip_names_the_mac_and_when_its_list_was_last_fresh() {
    let utc = TimeZone(identifier: "UTC")!
    #expect(GitText.unreachable(mac: "Studio", asOf: 1_700_000_000_000, timeZone: utc) == "Can't reach Studio · as of 22:13")
}

@Test func the_commit_button_pushes_after_unless_it_amends_or_is_busy() {
    #expect(GitText.commitButton(label: "Commit", amend: false, busy: false) == "Commit & Push")
    #expect(GitText.commitButton(label: "Commit", amend: true, busy: false) == "Amend")
    #expect(GitText.commitButton(label: "Committing…", amend: false, busy: true) == "Committing…")
}

@Suite("History days")
struct HistoryDaysTests {
    /// Tuesday 14 Nov 2023, 22:13 UTC.
    static let now = 1_700_000_000
    static let midnight = 1_699_920_000
    static let utc: Calendar = {
        var calendar = Calendar(identifier: .gregorian)
        calendar.timeZone = TimeZone(identifier: "UTC")!
        return calendar
    }()

    @Test("a commit's age reads in minutes, hours, yesterday, a weekday, then a date")
    func committed() {
        func said(_ ago: Int) -> String { GitText.committed(at: Self.now - ago, now: Self.now, calendar: Self.utc) }
        #expect(said(30) == "just now")
        #expect(said(720) == "12m ago")
        #expect(said(3 * 3_600) == "3h ago")
        #expect(GitText.committed(at: Self.midnight - 3_600, now: Self.now, calendar: Self.utc) == "yesterday")
        #expect(said(3 * 86_400) == "on Sat")
        #expect(said(10 * 86_400) == "on 4 Nov")
    }

    @Test("a day heads its commits as Today, Yesterday, then the weekday and date")
    func day() {
        func headed(_ ago: Int) -> String { GitText.day(at: Self.now - ago, now: Self.now, calendar: Self.utc) }
        #expect(headed(60) == "Today")
        #expect(GitText.day(at: Self.midnight - 60, now: Self.now, calendar: Self.utc) == "Yesterday")
        #expect(headed(3 * 86_400) == "Saturday 11 November")
    }

    @Test("an avatar takes two names' first letters, or one name's first two")
    func initials() {
        #expect(GitText.initials("Minh Ngo") == "MN")
        #expect(GitText.initials("minh") == "MI")
        #expect(GitText.initials("") == "")
    }

    @Test("history groups commits under a heading per day and marks each day's first and last")
    func rows() {
        func commit(_ sha: String, _ at: Int) -> LogCommit { LogCommit(sha: sha, parents: [], author: "A B", at: at, subject: sha) }
        let (a, b, c) = (commit("a", Self.now), commit("b", Self.now - 600), commit("c", Self.midnight - 60))
        let items = HistoryRows.items([a, b, c], now: Self.now, calendar: Self.utc)
        #expect(items.map(\.id) == ["day-a", "a", "b", "day-c", "c"])
        #expect(
            items.map(\.kind) == [
                .day("Today"), .commit(a, index: 0, first: true, last: false), .commit(b, index: 1, first: false, last: true),
                .day("Yesterday"), .commit(c, index: 2, first: true, last: true),
            ])
    }
}

@Test func tips_label_the_commits_they_point_at_by_name() {
    let tips = LogTips(head: "h", branch: "calm-otter", upstream: "u", base: "h")
    let head = GitHead(Sample.gitStatus(files: 1))
    #expect(HistoryScreen.labels("h", tips: tips, head: head).map(\.name) == ["calm-otter", "main"])
    #expect(HistoryScreen.labels("u", tips: tips, head: head).map(\.name) == ["origin/calm-otter"])
    #expect(HistoryScreen.labels("x", tips: tips, head: head).isEmpty)
}
