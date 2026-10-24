import Client
import Fixtures
import Testing
import Wire
@testable import Stores

@MainActor
private func store() -> GitHubStore {
    let s = GitHubStore()
    s.show(Sample.tree, GhPr(tree: Sample.tree, pr: Sample.pullRequest))
    return s
}

@Suite("GitHub store")
@MainActor
struct GitHubStoreTests {
    @Test("a tree's PR is one object for the life of the store, so only its screen redraws")
    func oneObject() {
        let s = store()
        #expect(s.pullRequest(Sample.tree) === s.pullRequest(Sample.tree))
        #expect(s.pullRequest(Sample.tree).pr?.number == 118)
        #expect(s.pullRequest(Sample.tree).loaded)
        #expect(!s.pullRequest("/elsewhere").loaded)
    }

    @Test("gh.changed is taken only for a tree the phone has open")
    func changed() {
        let s = store()
        #expect(s.apply(.ghChanged(.init(tree: Sample.tree))))
        #expect(!s.apply(.ghChanged(.init(tree: "/never/opened"))))
        #expect(!s.apply(.gitChanged(.init(tree: Sample.tree, rev: "r"))))
    }

    @Test("a missing or signed-out gh is the tree's problem, and no PR is no PR")
    func problem() {
        let s = GitHubStore()
        s.show("/a", GhPr(tree: "/a", problem: .signedOut))
        s.show("/b", GhPr(tree: "/b"))
        #expect(s.pullRequest("/a").problem == .signedOut && s.pullRequest("/a").pr == nil)
        #expect(s.pullRequest("/b").loaded && s.pullRequest("/b").pr == nil && s.pullRequest("/b").problem == nil)
    }

    @Test("a reply shows at once and a failed one is taken back out")
    func reply() async {
        let s = store()
        let before = s.pullRequest(Sample.tree).pr?.threads[0].comments.count
        await s.reply(Sample.tree, thread: "PRRT_a", body: "  Capped at 5.  ", at: 1)
        let t = s.pullRequest(Sample.tree)
        #expect(t.pr?.threads[0].comments.count == before)
        #expect(t.error == "Couldn't post the reply: Not connected to the Mac")
    }

    @Test("a resolve shows at once with Undo, and a failed one flips back")
    func resolve() async {
        let s = store()
        await s.resolve(Sample.tree, thread: "PRRT_a", resolved: true)
        let t = s.pullRequest(Sample.tree)
        #expect(t.pr?.threads[0].resolved == false)
        #expect(t.notice?.text == "Resolved the conversation")
        #expect(t.notice?.undo == .init(thread: "PRRT_a", resolved: false))
        #expect(t.error?.hasPrefix("Couldn't update the conversation") == true)
    }

    @Test("undo resolves back without offering another undo")
    func undo() async {
        let s = store()
        await s.resolve(Sample.tree, thread: "PRRT_c", resolved: false)
        #expect(s.pullRequest(Sample.tree).notice?.text == "Reopened the conversation")
        await s.undo(Sample.tree)
        #expect(s.pullRequest(Sample.tree).notice == nil)
    }

    @Test("a notice goes after a while, unless a newer one took its place")
    func noticeGoes() async throws {
        let s = store()
        s.noticeFor = .milliseconds(20)
        await s.resolve(Sample.tree, thread: "PRRT_a", resolved: true)
        try await Task.sleep(for: .milliseconds(200))
        #expect(s.pullRequest(Sample.tree).notice == nil)
    }

    @Test("a merge the Mac can't be asked for leaves the PR open and says why")
    func mergeOffline() async {
        let s = store()
        await s.merge(Sample.tree, method: .squash)
        let t = s.pullRequest(Sample.tree)
        #expect(t.pr?.state == "OPEN" && t.busy == nil)
        #expect(t.error == "Not connected to the Mac")
    }

    @Test("acting on a PR edits it as gh will report it next")
    func edits() {
        let pr = Sample.pullRequest
        #expect(pr.merged().state == "MERGED" && pr.merged().gate == .done)
        #expect(Sample.draftPullRequest.ready().draft == false && Sample.draftPullRequest.ready().gate == .checking)
        let c = ThreadComment(author: "me", body: "ok", at: 5)
        let replied = pr.replying(to: "PRRT_a", c)
        #expect(replied.threads[0].comments.last == c)
        #expect(replied.withoutReply(to: "PRRT_a", c) == pr)
        #expect(pr.resolving("PRRT_b", true).threads[1].resolved)
        #expect(pr.resolving("nope", true) == pr)
    }

    @Test("a failure says all gh or git said, since a push's first line only names the remote")
    func describesDetail() {
        let push = ServerError(message: "To github.com:acme/app.git", code: "git_failed",
                               detail: "To github.com:acme/app.git\n ! [rejected]        fix -> fix (non-fast-forward)")
        #expect(GitHubStore.describe(DaemonError.server(push)) == push.detail)
        #expect(GitHubStore.describe(DaemonError.server(ServerError(message: "Unknown project"))) == "Unknown project")
    }

    @Test("merge methods read as the desktop's menu")
    func methods() {
        #expect(MergeMethod.allCases.map(\.label) == ["Squash and merge", "Create a merge commit", "Rebase and merge"])
        #expect(MergeMethod.allCases.map(\.rawValue) == ["squash", "merge", "rebase"])
    }
}
