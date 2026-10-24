import Client
import Foundation
import Observation
import Wire

/// One tree's pull request as the PR screen shows it; its own object so a change to it redraws only its screen.
@MainActor
@Observable
public final class TreePullRequest {
    public let tree: String
    /// Whether gh has answered for this tree since it was opened.
    public internal(set) var loaded: Bool
    /// missing or signedOut: gh can't be asked at all.
    public internal(set) var problem: GhState?
    /// Nil once loaded means the branch has no PR.
    public internal(set) var pr: PullRequest?
    /// What the PR screen's button says while gh works: "Merging…".
    public internal(set) var busy: String?
    public var error: String?
    public internal(set) var notice: PullRequestNotice?

    public init(tree: String, loaded: Bool = false, problem: GhState? = nil, pr: PullRequest? = nil) {
        self.tree = tree
        self.loaded = loaded
        self.problem = problem
        self.pr = pr
    }
}

/// A short note after acting on a PR, with the resolve it can take back.
public struct PullRequestNotice: Hashable, Sendable, Identifiable {
    public let id: Int
    public let text: String
    public let undo: Undo?

    public struct Undo: Hashable, Sendable {
        public let thread: String
        /// What Undo sets the thread to.
        public let resolved: Bool
    }
}

/// PRs, checks and review threads of one Mac's trees.
@MainActor
@Observable
public final class GitHubStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?
    public private(set) var status: GhStatus?
    /// Each project's PRs, by project path.
    public private(set) var lists: [String: [PrItem]] = [:]
    @ObservationIgnored private var trees: [String: TreePullRequest] = [:]
    @ObservationIgnored private var notices = 0
    /// How long a notice with Undo stays, as the desktop's.
    @ObservationIgnored var noticeFor: Duration = .seconds(4)

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    /// The PR of a tree, made on first ask.
    public func pullRequest(_ tree: String) -> TreePullRequest {
        if let t = trees[tree] { return t }
        let t = TreePullRequest(tree: tree)
        trees[tree] = t
        return t
    }

    /// Shows a PR without asking the Mac, as the gallery and previews do.
    public func show(_ tree: String, _ answer: GhPr) {
        take(answer, into: pullRequest(tree))
    }

    public func showList(_ project: String, _ items: [PrItem]) {
        lists[project] = items
    }

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .ghChanged(let m):
            guard let t = trees[m.tree] else { return false }
            Task { await refresh(t.tree) }
            return true
        case .ghStatus(let s):
            status = s
            return true
        default:
            return false
        }
    }

    public func connected(_ hello: HelloOk) {
        guard hello.has(Caps.gh) else { return }
        Task { await checkStatus() }
        for tree in trees.keys { Task { await refresh(tree) } }
    }

    /// Asks the Mac for the tree's PR; it answers from its cache while that is fresh.
    public func refresh(_ tree: String) async {
        let t = pullRequest(tree)
        do {
            guard case .ghPr(let answer) = try await request(.ghPr(.init(tree: tree))) else { return }
            take(answer, into: t)
        } catch {
            if !t.loaded { t.error = Self.describe(error) }
        }
    }

    private func take(_ answer: GhPr, into t: TreePullRequest) {
        t.loaded = true
        t.problem = answer.problem
        if t.pr != answer.pr { t.pr = answer.pr }
        t.error = nil
    }

    public func checkStatus(tree: String? = nil) async {
        if case .ghStatus(let s)? = try? await request(.ghStatus(.init(tree: tree))) { status = s }
    }

    public func refreshList(_ project: String) async throws {
        if case .ghPrs(let list) = try await request(.ghPrs(.init(project: project))) { lists[project] = list.items }
    }

    /// Pushes the tree's branch and opens its PR; the screen shows it at once.
    public func create(_ tree: String, title: String, body: String, draft: Bool, base: String?) async throws -> GhPrCreated {
        let t = pullRequest(tree)
        t.busy = draft ? "Creating draft PR…" : "Creating PR…"
        defer { t.busy = nil }
        let reply = try await request(.ghPrCreate(.init(tree: tree, title: title, body: body, draft: draft, base: base)), timeout: .seconds(180))
        guard case .ghPrCreated(let made) = reply else { throw DaemonError.encoding("gh.pr.created expected") }
        say((draft ? "Opened draft PR #\(made.number)" : "Opened PR #\(made.number)") + (base.map { " into \($0)" } ?? ""), in: t)
        await refresh(tree)
        return made
    }

    public func merge(_ tree: String, method: MergeMethod) async {
        let t = pullRequest(tree)
        guard let pr = t.pr else { return }
        await act(t, busy: "Merging…") {
            _ = try await self.request(.ghPrMerge(.init(tree: tree, number: pr.number, method: method.rawValue)), timeout: .seconds(90))
            t.pr = pr.merged()
            self.say("Merged PR #\(pr.number) into \(pr.base)", in: t)
        }
    }

    public func markReady(_ tree: String) async {
        let t = pullRequest(tree)
        guard let pr = t.pr else { return }
        await act(t, busy: "Marking ready…") {
            _ = try await self.request(.ghPrReady(.init(tree: tree, number: pr.number)), timeout: .seconds(90))
            t.pr = pr.ready()
        }
    }

    /// Shows the reply at once as `me`'s; a failure takes it back out.
    public func reply(_ tree: String, thread: String, body: String, at: Int = Int(Date().timeIntervalSince1970)) async {
        let t = pullRequest(tree)
        let body = body.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !body.isEmpty, let pr = t.pr else { return }
        let me = status?.login ?? "You"
        t.pr = pr.replying(to: thread, ThreadComment(author: me, body: body, at: at))
        do {
            _ = try await request(.ghThreadReply(.init(tree: tree, thread: thread, body: body)), timeout: .seconds(60))
        } catch {
            t.pr = t.pr?.withoutReply(to: thread, ThreadComment(author: me, body: body, at: at))
            t.error = "Couldn't post the reply: \(Self.describe(error))"
        }
    }

    /// Resolves or reopens a thread at once, with Undo unless this is the undo; a failure flips it back.
    public func resolve(_ tree: String, thread: String, resolved: Bool, undoable: Bool = true) async {
        let t = pullRequest(tree)
        guard let pr = t.pr else { return }
        t.pr = pr.resolving(thread, resolved)
        if undoable {
            say(resolved ? "Resolved the conversation" : "Reopened the conversation", in: t, undo: .init(thread: thread, resolved: !resolved))
        } else {
            t.notice = nil
        }
        do {
            _ = try await request(.ghThreadResolve(.init(tree: tree, thread: thread, resolved: resolved)), timeout: .seconds(60))
        } catch {
            t.pr = t.pr?.resolving(thread, !resolved)
            t.error = "Couldn't update the conversation: \(Self.describe(error))"
        }
    }

    public func undo(_ tree: String) async {
        let t = pullRequest(tree)
        guard let undo = t.notice?.undo else { return }
        await resolve(tree, thread: undo.thread, resolved: undo.resolved, undoable: false)
    }

    private func act(_ t: TreePullRequest, busy: String, _ body: () async throws -> Void) async {
        guard t.busy == nil else { return }
        t.busy = busy
        t.error = nil
        defer { t.busy = nil }
        do { try await body() } catch { t.error = Self.describe(error) }
    }

    private func say(_ text: String, in t: TreePullRequest, undo: PullRequestNotice.Undo? = nil) {
        notices += 1
        let id = notices
        t.notice = PullRequestNotice(id: id, text: text, undo: undo)
        let wait = noticeFor
        Task { [weak t] in
            try? await Task.sleep(for: wait)
            if t?.notice?.id == id { t?.notice = nil }
        }
    }

    private func request(_ message: ClientMessage, timeout: Duration? = nil) async throws -> ServerMessage {
        guard let connection else { throw DaemonError.offline }
        return try await connection.request(message, timeout: timeout)
    }

    /// What a failed request says on screen: the Mac's message, else why it never got there.
    public nonisolated static func describe(_ error: any Error) -> String {
        switch error as? DaemonError {
        case .server(let e)?: e.message
        case .offline?: "Not connected to the Mac"
        case .timedOut?: "The Mac didn't answer in time"
        case .closed?: "The connection dropped"
        default: "\(error)"
        }
    }
}

/// How a PR merges, as `gh pr merge` names it.
public enum MergeMethod: String, CaseIterable, Sendable {
    case squash, merge, rebase

    /// The desktop's labels (github.rs `Method::label`).
    public var label: String {
        switch self {
        case .squash: "Squash and merge"
        case .merge: "Create a merge commit"
        case .rebase: "Rebase and merge"
        }
    }
}

extension PullRequest {
    func merged() -> PullRequest {
        var pr = self
        pr.state = "MERGED"
        pr.gate = .done
        return pr
    }

    func ready() -> PullRequest {
        var pr = self
        pr.draft = false
        // gh's next answer says what holds it back now; until then it is checked.
        if pr.gate == .draft { pr.gate = .checking }
        return pr
    }

    func replying(to thread: String, _ comment: ThreadComment) -> PullRequest {
        var pr = self
        if let i = pr.threads.firstIndex(where: { $0.id == thread }) { pr.threads[i].comments.append(comment) }
        return pr
    }

    func withoutReply(to thread: String, _ comment: ThreadComment) -> PullRequest {
        var pr = self
        if let i = pr.threads.firstIndex(where: { $0.id == thread }), let j = pr.threads[i].comments.lastIndex(of: comment) {
            pr.threads[i].comments.remove(at: j)
        }
        return pr
    }

    func resolving(_ thread: String, _ resolved: Bool) -> PullRequest {
        var pr = self
        if let i = pr.threads.firstIndex(where: { $0.id == thread }) { pr.threads[i].resolved = resolved }
        return pr
    }
}
