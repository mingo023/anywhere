import Client
import DiffModel
import Observation
import Wire

/// Status, diffs, log and commits of the trees one Mac shows. Each tree's changes and history are their own small
/// observable objects, so a refresh of one redraws only its screen.
@MainActor
@Observable
public final class GitStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?
    @ObservationIgnored private var changes: [String: TreeChanges] = [:]
    @ObservationIgnored private var histories: [String: TreeHistory] = [:]
    /// How many screens show each tree; the watch set is the trees with any.
    @ObservationIgnored private var shown: [String: Int] = [:]
    @ObservationIgnored private var sentWatch: Set<String> = []
    /// The last git.watch sent; each waits for the one before, so the Mac ends on the newest set.
    @ObservationIgnored private var watchSend: Task<Void, Never>?
    @ObservationIgnored private var diffs = DiffCache(capacity: 24)
    @ObservationIgnored private var commits: [String: CommitDetail] = [:]

    public init(connection: DaemonConnection? = nil) {
        self.connection = connection
    }

    // MARK: HostStore

    public func apply(_ message: ServerMessage) -> Bool {
        guard case .gitChanged(let moved) = message, let tree = changes[moved.tree] else { return false }
        guard tree.head?.rev != moved.rev else { return false }
        tree.stale = true
        if connection != nil, shown[moved.tree, default: 0] > 0 { Task { await refresh(moved.tree) } }
        return true
    }

    public func connected(_ hello: HelloOk) {
        sentWatch = []
        syncWatch()
        for tree in watchSet { Task { await refresh(tree) } }
    }

    public func disconnected() {
        for tree in changes.values where tree.busy != nil { tree.busy = nil }
    }

    // MARK: Trees on screen

    public func changes(_ tree: String) -> TreeChanges {
        if let t = changes[tree] { return t }
        let t = TreeChanges(tree: tree)
        changes[tree] = t
        return t
    }

    public func history(_ tree: String) -> TreeHistory {
        if let h = histories[tree] { return h }
        let h = TreeHistory(tree: tree)
        histories[tree] = h
        return h
    }

    /// The trees a screen shows now; pocketd polls only these.
    public var watchSet: Set<String> { Set(shown.filter { $0.value > 0 }.keys) }

    /// A screen showing `tree` appeared; its status loads and the Mac starts watching it.
    public func show(_ tree: String) {
        shown[tree, default: 0] += 1
        syncWatch()
        let t = changes(tree)
        if t.head == nil || t.stale { Task { await refresh(tree) } }
    }

    public func hide(_ tree: String) {
        guard let n = shown[tree] else { return }
        shown[tree] = n > 1 ? n - 1 : nil
        syncWatch()
    }

    private func syncWatch() {
        let now = watchSet
        guard now != sentWatch, let connection else { return }
        sentWatch = now
        let previous = watchSend
        watchSend = Task {
            await previous?.value
            _ = try? await connection.request(.gitWatch(.init(trees: now.sorted())))
        }
    }

    // MARK: Reads

    /// Reads the first page again; a read that a newer write overtook is dropped.
    public func refresh(_ tree: String) async {
        let t = changes(tree)
        let generation = t.generation
        t.loading = true
        defer { t.loading = false }
        do {
            let page = try await statusPage(tree, cursor: 0)
            t.take(page, cursor: 0, generation: generation)
        } catch {
            t.error = Self.describe(error)
        }
    }

    /// The next page of files, when the list scrolls near its end.
    public func loadMore(_ tree: String) async {
        let t = changes(tree)
        guard let cursor = t.nextCursor, !t.loading else { return }
        let generation = t.generation
        t.loading = true
        defer { t.loading = false }
        do {
            t.take(try await statusPage(tree, cursor: cursor), cursor: cursor, generation: generation)
        } catch {
            t.error = Self.describe(error)
        }
    }

    private func statusPage(_ tree: String, cursor: Int) async throws -> GitStatus {
        guard let connection else { throw DaemonError.offline }
        guard case .gitStatus(let page) = try await connection.request(.gitStatus(.init(tree: tree, cursor: cursor > 0 ? cursor : nil))) else {
            throw DaemonError.encoding("git.status")
        }
        return page
    }

    /// A diff ready to draw, from the cache while the tree's rev holds. Rows are built off the main actor.
    public func diff(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool = false) async throws -> DiffDocument {
        let key = diffKey(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace)
        if let hit = diffs[key] { return hit }
        guard let connection else { throw DaemonError.offline }
        let reply = try await connection.request(.gitDiff(.init(tree: tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace ? true : nil)))
        guard case .gitDiff(let diff) = reply else { throw DaemonError.encoding("git.diff") }
        let doc = await Task.detached(priority: .userInitiated) { DiffDocument(diff) }.value
        diffs[key] = doc
        return doc
    }

    /// The diff already in the cache, so a screen can draw it on its first frame.
    public func cachedDiff(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool = false) -> DiffDocument? {
        diffs[diffKey(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace)]
    }

    private func diffKey(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool) -> DiffKey {
        DiffKey(tree: tree, path: path, at: at, rev: Self.revKey(at, rev: changes[tree]?.head?.rev), ignoreWhitespace: ignoreWhitespace)
    }

    /// A commit's diffs never change; the working tree's change with its rev, and a base diff with HEAD.
    nonisolated static func revKey(_ at: DiffAt, rev: String?) -> String {
        switch at {
        case .commit: ""
        case .working, .staged, .base: rev ?? ""
        }
    }

    /// The next page of history, continuing the graph's lanes.
    public func loadHistory(_ tree: String, all: Bool = false) async {
        let h = history(tree)
        if h.all != all { h.reset(all: all) }
        guard h.more, !h.loading, let connection else { return }
        h.loading = true
        defer { h.loading = false }
        do {
            // A page whose tips moved empties the list; read the first page again, or the screen has no row to ask for more.
            repeat {
                let skip = h.commits.count
                guard case .gitLog(let log) = try await connection.request(.gitLog(.init(tree: tree, all: all ? true : nil, skip: skip > 0 ? skip : nil)))
                else { return }
                h.take(log, skip: skip)
            } while h.commits.isEmpty && h.more
        } catch {
            h.error = Self.describe(error)
        }
    }

    public func cachedCommit(_ sha: String) -> CommitDetail? { commits[sha] }

    public func commit(_ tree: String, sha: String) async throws -> CommitDetail {
        if let hit = commits[sha] { return hit }
        guard let connection else { throw DaemonError.offline }
        guard case .gitCommit(let detail) = try await connection.request(.gitCommit(.init(tree: tree, sha: sha))) else {
            throw DaemonError.encoding("git.commit")
        }
        commits[sha] = detail
        return detail
    }

    // MARK: Replies taken as they are, for previews and the gallery

    /// Takes a whole status as its tree's first page.
    public func take(_ status: GitStatus) {
        let t = changes(status.tree)
        t.take(status, cursor: 0, generation: t.generation)
    }

    public func take(_ log: GitLog, tree: String) {
        history(tree).take(log, skip: 0)
    }

    public func take(_ diff: GitDiff, at: DiffAt) {
        diffs[diffKey(diff.tree, path: diff.path, at: at, ignoreWhitespace: false)] = DiffDocument(diff)
    }

    public func take(_ commit: CommitDetail) {
        commits[commit.sha] = commit
    }

    // MARK: Writes

    /// Flips the rows at once, as the desktop does, then asks the Mac; the next read settles it.
    public func stage(_ tree: String, paths: [String], staged: Bool) async {
        let t = changes(tree)
        t.flip(paths, staged: staged)
        await write(tree, busy: nil) { try await $0.request(.gitStage(.init(tree: tree, paths: paths, staged: staged))) }
    }

    public func discard(_ tree: String, paths: [String]) async {
        await write(tree, busy: nil) { try await $0.request(.gitDiscard(.init(tree: tree, paths: paths))) }
    }

    /// Commits, then pushes when asked; true when everything asked for went through.
    @discardableResult
    public func commit(_ tree: String, message: String, kind: CommitKind, all: Bool) async -> Bool {
        let busy = kind == .amend ? "Amending…" : "Committing…"
        let committed = await write(tree, busy: busy) {
            try await $0.request(.gitCommitCreate(.init(tree: tree, message: message, amend: kind == .amend ? true : nil, all: all ? true : nil)), timeout: .seconds(120))
        }
        guard committed, kind == .push else { return committed }
        return await push(tree)
    }

    @discardableResult
    public func push(_ tree: String, lease: Bool = false) async -> Bool {
        await write(tree, busy: "Pushing…") { try await $0.request(.gitPush(.init(tree: tree, lease: lease ? true : nil)), timeout: .seconds(150)) }
    }

    @discardableResult
    public func pull(_ tree: String) async -> Bool {
        await write(tree, busy: "Pulling…") { try await $0.request(.gitPull(.init(tree: tree)), timeout: .seconds(150)) }
    }

    @discardableResult
    private func write(_ tree: String, busy: String?, _ send: (DaemonConnection) async throws -> ServerMessage) async -> Bool {
        let t = changes(tree)
        guard let connection else {
            t.error = Self.describe(DaemonError.offline)
            return false
        }
        if let busy { t.busy = busy }
        t.error = nil
        defer {
            if busy != nil { t.busy = nil }
        }
        do {
            _ = try await send(connection)
            await refresh(tree)
            return true
        } catch {
            t.error = Self.describe(error)
            await refresh(tree)
            return false
        }
    }

    /// The Mac's own words for a refused request, else what went wrong in a few.
    public nonisolated static func describe(_ error: any Error) -> String {
        switch error as? DaemonError {
        case .server(let e)?: e.message
        case .offline?: "The Mac is offline"
        case .timedOut?: "The Mac didn't answer in time"
        default: "Something went wrong"
        }
    }
}

/// A tree's branch line: everything of a status but its files.
public struct GitHead: Hashable, Sendable {
    public var rev: String
    public var branch: String?
    public var detached: Bool
    public var upstream: String?
    public var base: String?
    public var ahead: Int
    public var behind: Int
    public var unpushed: Int?
    public var totals: GitTotals
    public var commits: [CommitRef]

    public init(_ s: GitStatus) {
        rev = s.rev
        branch = s.branch
        detached = s.detached ?? false
        upstream = s.upstream
        base = s.base
        ahead = s.ahead
        behind = s.behind
        unpushed = s.unpushed
        totals = s.totals
        commits = s.commits
    }
}

/// One tree's changes as the Changes screen shows them, page by page.
@MainActor
@Observable
public final class TreeChanges {
    public let tree: String
    public internal(set) var head: GitHead?
    public internal(set) var files: [GitFile] = []
    public internal(set) var nextCursor: Int?
    public internal(set) var loading = false
    /// What git is doing now: "Committing…", "Pushing…".
    public internal(set) var busy: String?
    public var error: String?
    /// The Mac said the tree moved since the last read.
    public internal(set) var stale = false
    /// Bumped by each optimistic change, so a read that started before it can't undo it.
    @ObservationIgnored var generation = 0

    init(tree: String) { self.tree = tree }

    /// Takes in a page: the first replaces everything, a later one extends the list only while the rev holds.
    func take(_ page: GitStatus, cursor: Int, generation: Int) {
        guard generation == self.generation else { return }
        if cursor == 0 {
            head = GitHead(page)
            files = page.files
            stale = false
        } else {
            guard page.rev == head?.rev, cursor == files.count else { return }
            files.append(contentsOf: page.files)
        }
        nextCursor = page.nextCursor
    }

    func flip(_ paths: [String], staged: Bool) {
        generation += 1
        let before = files.count(where: \.staged)
        ChangeTree.flipStaged(&files, paths: paths, staged: staged)
        head?.totals.staged += files.count(where: \.staged) - before
    }

    /// Every changed file is loaded, so section-wide actions see them all.
    public var complete: Bool { nextCursor == nil }
}

/// One tree's history with its graph laid out, page after page.
@MainActor
@Observable
public final class TreeHistory {
    public let tree: String
    public internal(set) var all = false
    public internal(set) var tips = LogTips()
    public internal(set) var commits: [LogCommit] = []
    public internal(set) var layout = GraphLayout(tips: LogTips())
    public internal(set) var more = true
    public internal(set) var loading = false
    public var error: String?

    init(tree: String) { self.tree = tree }

    func reset(all: Bool) {
        self.all = all
        commits = []
        layout = GraphLayout(tips: LogTips())
        more = true
    }

    /// Takes in a page; one that doesn't start where the list ends, or whose tips moved, starts the graph over.
    func take(_ log: GitLog, skip: Int) {
        if skip == 0 || log.tips != tips || skip != commits.count {
            tips = log.tips
            commits = []
            layout = GraphLayout(tips: log.tips)
            if skip != 0 {
                more = true
                return
            }
        }
        commits.append(contentsOf: log.commits)
        layout.append(log.commits)
        more = log.more
    }
}

struct DiffKey: Hashable {
    var tree: String
    var path: String
    var at: DiffAt
    var rev: String
    var ignoreWhitespace: Bool
}

/// The last few diffs opened, so flicking between files doesn't refetch them.
struct DiffCache {
    let capacity: Int
    private var entries: [DiffKey: DiffDocument] = [:]
    private var order: [DiffKey] = []

    init(capacity: Int) { self.capacity = capacity }

    subscript(key: DiffKey) -> DiffDocument? {
        get { entries[key] }
        set {
            entries[key] = newValue
            order.removeAll { $0 == key }
            guard newValue != nil else { return }
            order.append(key)
            if order.count > capacity { entries[order.removeFirst()] = nil }
        }
    }
}
