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
