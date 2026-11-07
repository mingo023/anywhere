import Client
import DiffModel
import Fixtures
import Testing
import Wire
@testable import Stores

private func page(_ status: GitStatus, from: Int, count: Int) -> GitStatus {
    var p = status
    p.files = Array(status.files[from..<min(from + count, status.files.count)])
    p.nextCursor = from + count < status.files.count ? from + count : nil
    return p
}

@Suite("Git store")
@MainActor
struct GitStoreTests {
    @Test("the first page replaces the list and later pages extend it while the rev holds")
    func pages() {
        let store = GitStore()
        let t = store.changes(Sample.tree)
        let all = Sample.gitStatus(files: 1200)
        t.take(page(all, from: 0, count: 500), cursor: 0, generation: 0)
        #expect(t.files.count == 500 && t.nextCursor == 500 && !t.complete)
        var moved = page(all, from: 500, count: 500)
        moved.rev = "other"
        t.take(moved, cursor: 500, generation: 0)
        #expect(t.files.count == 500)
        t.take(page(all, from: 500, count: 500), cursor: 500, generation: 0)
        t.take(page(all, from: 1000, count: 500), cursor: 1000, generation: 0)
        #expect(t.files == all.files && t.complete && t.head?.totals.files == 1200)
    }

    @Test("a refresh's pages replace the loaded range at once, keeping every row past the first page")
    func refreshPages() {
        let store = GitStore()
        let t = store.changes(Sample.tree)
        let all = Sample.gitStatus(files: 1200)
        t.take(page(all, from: 0, count: 500), cursor: 0, generation: 0)
        t.take(page(all, from: 500, count: 500), cursor: 500, generation: 0)
        let revision = t.revision
        var edited = all
        edited.files[700].added = 999
        t.take([page(edited, from: 0, count: 1000)], generation: 0)
        #expect(t.files.count == 1000 && t.files[700].added == 999 && t.nextCursor == 1000)
        #expect(t.revision == revision + 1)
    }

    @Test("section counts come from the totals, not the loaded page")
    func counts() {
        let store = GitStore()
        let t = store.changes(Sample.tree)
        var all = Sample.gitStatus(files: 1200)
        all.totals.unstaged = 900
        t.take(page(all, from: 0, count: 500), cursor: 0, generation: 0)
        #expect(t.count(.staged) == all.totals.staged && t.count(.changes) == 900)
        all.totals.unstaged = nil
        t.take(page(all, from: 0, count: 500), cursor: 0, generation: 0)
        #expect(t.count(.changes) == t.files.count(where: \.unstaged))
    }

    @Test("a stage moves its files between both section counts at once")
    func flipCounts() {
        let store = GitStore()
        let t = store.changes(Sample.tree)
        var status = Sample.gitStatus(files: 6)
        status.totals.unstaged = status.files.count(where: \.unstaged)
        t.take(status, cursor: 0, generation: 0)
        let paths = status.files.filter { $0.unstaged && !$0.staged }.map(\.path)
        t.flip(paths, staged: true)
        #expect(t.count(.staged) == status.totals.staged + paths.count)
        #expect(t.count(.changes) == status.totals.unstaged! - paths.count)
    }

    @Test("a fresh first page of history keeps the loaded pages while the tips hold, and starts over when they moved")
    func historyRefresh() {
        let store = GitStore()
        let h = store.history(Sample.tree)
        let log = Sample.log(commits: 100)
        h.take(GitLog(tips: log.tips, commits: Array(log.commits[..<50]), more: true), skip: 0)
        h.take(GitLog(tips: log.tips, commits: Array(log.commits[50...]), more: false), skip: 50)
        h.takeFirst(GitLog(tips: log.tips, commits: Array(log.commits[..<50]), more: true))
        #expect(h.commits.count == 100)
        var moved = log.tips
        moved.head = "new"
        h.takeFirst(GitLog(tips: moved, commits: Array(log.commits[..<50]), more: true))
        #expect(h.commits.count == 50 && h.tips == moved && h.more)
    }

    @Test("a compare's first page replaces it and later pages extend it where it ends")
    func comparePages() {
        let store = GitStore()
        let files = (0..<3).map { CommitFile(path: "f\($0)", status: .modified, added: 1, removed: 0) }
        let totals = CompareTotals(files: 3, added: 3, removed: 0)
        store.take(GitCompare(tree: Sample.tree, against: "main", totals: totals, files: Array(files[..<2]), nextCursor: 2))
        let c = store.compare(Sample.tree, against: "main")
        c.take(GitCompare(tree: Sample.tree, against: "main", totals: totals, files: [files[2]]), cursor: 5)
        #expect(c.files.count == 2 && c.nextCursor == 2)
        c.take(GitCompare(tree: Sample.tree, against: "main", totals: totals, files: [files[2]]), cursor: 2)
        #expect(c.files == files && c.nextCursor == nil && c.totals == totals)
        #expect(store.compare(Sample.tree, against: "dev").files.isEmpty)
    }

    @Test("a read that started before a stage can't undo its optimistic flip")
    func flip() {
        let store = GitStore()
        let t = store.changes(Sample.tree)
        let status = Sample.gitStatus(files: 6)
        t.take(status, cursor: 0, generation: 0)
        let path = status.files.first { !$0.staged }!.path
        let before = t.generation
        t.flip([path], staged: true)
        t.take(status, cursor: 0, generation: before)
        #expect(t.files.first { $0.path == path }?.staged == true)
        t.take(status, cursor: 0, generation: t.generation)
        #expect(t.files.first { $0.path == path }?.staged == false)
    }

    @Test("git.changed marks a known tree stale only when its rev moved")
    func changed() {
        let store = GitStore()
        #expect(!store.apply(.gitChanged(.init(tree: Sample.tree, rev: "x"))))
        let t = store.changes(Sample.tree)
        t.take(Sample.gitStatus(files: 3), cursor: 0, generation: 0)
        #expect(!store.apply(.gitChanged(.init(tree: Sample.tree, rev: t.head!.rev))))
        #expect(!t.stale)
        #expect(store.apply(.gitChanged(.init(tree: Sample.tree, rev: "moved"))))
        #expect(t.stale)
        #expect(!store.apply(.ghChanged(.init(tree: Sample.tree))))
    }

    @Test("the watch set holds a tree while any screen shows it")
    func watch() {
        let store = GitStore()
        store.show("/a")
        store.show("/a")
        store.show("/b")
        store.hide("/a")
        #expect(store.watchSet == ["/a", "/b"])
        store.hide("/a")
        store.hide("/b")
        store.hide("/b")
        #expect(store.watchSet.isEmpty)
    }

    @Test("history pages continue the graph, and moved tips start it over")
    func history() {
        let store = GitStore()
        let h = store.history(Sample.tree)
        let log = Sample.log(commits: 100)
        h.take(GitLog(tips: log.tips, commits: Array(log.commits[..<50]), more: true), skip: 0)
        h.take(GitLog(tips: log.tips, commits: Array(log.commits[50...]), more: false), skip: 50)
        var whole = GraphLayout(tips: log.tips)
        whole.append(log.commits)
        #expect(h.commits.count == 100 && h.layout.rows == whole.rows && !h.more)
        var moved = log.tips
        moved.head = "new"
        h.take(GitLog(tips: moved, commits: Array(log.commits[..<50]), more: true), skip: 100)
        #expect(h.commits.isEmpty && h.more && h.tips == moved)
    }

    @Test("a commit's diff is cached for good, a working one only for its rev")
    func revKeys() {
        #expect(GitStore.revKey(.commit(rev: "abc"), rev: "r1") == "")
        #expect(GitStore.revKey(.working, rev: "r1") == "r1")
        #expect(GitStore.revKey(.staged, rev: nil) == "")
    }

    @Test("the diff cache keeps the newest diffs")
    func cache() {
        var cache = DiffCache(capacity: 2)
        let doc = DiffDocument(Sample.smallDiff)
        let keys = (0..<3).map { DiffKey(tree: "/t", path: "p\($0)", at: .working, rev: "r", ignoreWhitespace: false) }
        for k in keys { cache[k] = doc }
        #expect(cache[keys[0]] == nil && cache[keys[1]] != nil && cache[keys[2]] != nil)
        cache[keys[1]] = doc
        cache[DiffKey(tree: "/t", path: "p3", at: .working, rev: "r", ignoreWhitespace: false)] = doc
        #expect(cache[keys[1]] != nil && cache[keys[2]] == nil)
    }

    @Test("a refusal reads as the Mac's own words")
    func describe() {
        #expect(GitStore.describe(DaemonError.server(ServerError(message: "! [rejected] main -> main (non-fast-forward)", code: .gitFailed))) == "! [rejected] main -> main (non-fast-forward)")
    }
}
