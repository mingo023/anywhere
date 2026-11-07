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
    @ObservationIgnored private var compares: [CompareKey: TreeCompare] = [:]
    /// The Mac's name, as errors say it; its hostname once it has said hello.
    @ObservationIgnored public private(set) var mac: String

    public init(connection: DaemonConnection? = nil, mac: String = FilesStore.unnamed) {
        self.connection = connection
        self.mac = mac
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
        mac = hello.hostname
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

    /// A screen showing `tree` appeared; its status loads and the Mac starts watching it. `reread` reads it even when
    /// cached: an unwatched tree gets no git.changed, so its cached status may be old.
    public func show(_ tree: String, reread: Bool = false) {
        shown[tree, default: 0] += 1
        syncWatch()
        let t = changes(tree)
        if t.head == nil || t.stale || reread { Task { await refresh(tree) } }
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

    /// Reads the pages already loaded again and swaps them in at once, so rows keep their place; a read that a newer
    /// write overtook is dropped.
    public func refresh(_ tree: String) async {
        let t = changes(tree)
        let generation = t.generation
        t.loading = true
        defer { t.loading = false }
        do {
            t.take(try await statusPages(tree, upTo: t.files.count), generation: generation)
        } catch {
            t.error = describe(error)
        }
    }

    /// Reads every page not loaded yet, so a section-wide action sees every file; false when that couldn't be done.
    public func complete(_ tree: String) async -> Bool {
        let t = changes(tree)
        guard !t.complete else { return true }
        let generation = t.generation
        do {
            t.take(try await statusPages(tree, upTo: .max), generation: generation)
        } catch {
            t.error = describe(error)
            return false
        }
        if !t.complete { t.error = "The changes moved while they were read. Try again." }
        return t.complete
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
            t.error = describe(error)
        }
    }

    /// Pages from the first until `count` files are read or the list ends. pocketd serves one read's pages for a while,
    /// so they agree; a page from a newer read ends the run.
    private func statusPages(_ tree: String, upTo count: Int) async throws -> [GitStatus] {
        let limit = count > Limits.gitStatusPage ? min(count, Limits.gitStatusPageMax) : nil
        var pages = [try await statusPage(tree, cursor: 0, limit: limit)]
        var read = pages[0].files.count
        while read < count, let cursor = pages.last?.nextCursor {
            let page = try await statusPage(tree, cursor: cursor, limit: limit)
            guard page.rev == pages[0].rev else { break }
            pages.append(page)
            read += page.files.count
        }
        return pages
    }

    private func statusPage(_ tree: String, cursor: Int, limit: Int? = nil) async throws -> GitStatus {
        guard let connection else { throw DaemonError.offline }
        let reply = try await connection.request(.gitStatus(.init(tree: tree, cursor: cursor > 0 ? cursor : nil, limit: limit)))
        guard case .gitStatus(let page) = reply else { throw DaemonError.encoding("git.status") }
        return page
    }

    /// A diff ready to draw, from the cache while the tree's rev holds. Rows are built off the main actor. `context` is
    /// how many unchanged lines show around each change; nil is git's 3.
    public func diff(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool = false, context: Int? = nil) async throws -> DiffDocument {
        let key = diffKey(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace, context: context)
        if let hit = diffs[key] { return hit }
        guard let connection else { throw DaemonError.offline }
        let reply = try await connection.request(
            .gitDiff(.init(tree: tree, path: path, at: at, context: context, ignoreWhitespace: ignoreWhitespace ? true : nil)))
        guard case .gitDiff(let diff) = reply else { throw DaemonError.encoding("git.diff") }
        let doc = await Task.detached(priority: .userInitiated) { DiffDocument(diff) }.value
        diffs[key] = doc
        return doc
    }

    /// The diff already in the cache, so a screen can draw it on its first frame.
    public func cachedDiff(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool = false, context: Int? = nil) -> DiffDocument? {
        diffs[diffKey(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace, context: context)]
    }

    private func diffKey(_ tree: String, path: String, at: DiffAt, ignoreWhitespace: Bool, context: Int?) -> DiffKey {
        DiffKey(
            tree: tree, path: path, at: at, rev: Self.revKey(at, rev: changes[tree]?.head?.rev), ignoreWhitespace: ignoreWhitespace,
            context: context)
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
        do {
            try await readHistory(h, all: all, connection)
        } catch {
            h.error = describe(error)
        }
        h.loading = false
    }

    /// Kept out of `loadHistory`: the loop inside its `defer` and `do` crashed the 6.2 optimizer in release builds.
    private func readHistory(_ h: TreeHistory, all: Bool, _ connection: DaemonConnection) async throws {
        // A page whose tips moved empties the list; read the first page again, or the screen has no row to ask for more.
        repeat {
            let skip = h.commits.count
            guard case .gitLog(let log) = try await connection.request(.gitLog(.init(tree: h.tree, all: all ? true : nil, skip: skip > 0 ? skip : nil)))
            else { return }
            h.take(log, skip: skip)
        } while h.commits.isEmpty && h.more
    }

    /// Reads the first page of history again, as after a commit; the pages loaded stay while the tips hold.
    public func refreshHistory(_ tree: String) async {
        let h = history(tree)
        guard !h.loading, let connection else { return }
        h.loading = true
        do {
            if case .gitLog(let log) = try await connection.request(.gitLog(.init(tree: tree, all: h.all ? true : nil))) { h.takeFirst(log) }
        } catch {
            h.error = describe(error)
        }
        h.loading = false
    }

    /// What `tree` committed since it left `against`.
    public func compare(_ tree: String, against: String) -> TreeCompare {
        let key = CompareKey(tree: tree, against: against)
        if let c = compares[key] { return c }
        let c = TreeCompare(tree: tree, against: against)
        compares[key] = c
        return c
    }

    /// Reads a compare's first page again, or with `more` its next one. A first-page read overtakes any read under way.
    public func loadCompare(_ tree: String, against: String, more: Bool = false) async {
        let c = compare(tree, against: against)
        guard let cursor = more ? c.nextCursor : 0, !(more && c.loading) else { return }
        guard let connection else {
            c.error = describe(DaemonError.offline)
            return
        }
        if cursor == 0 { c.generation += 1 }
        let generation = c.generation
        c.loading = true
        do {
            let reply = try await connection.request(.gitCompare(.init(tree: tree, against: against, cursor: cursor > 0 ? cursor : nil)))
            guard case .gitCompare(let page) = reply else { throw DaemonError.encoding("git.compare") }
            c.take(page, cursor: cursor, generation: generation)
        } catch DaemonError.server(let e) where e.code == "unknown_ref" {
            if generation == c.generation { c.missing = true }
        } catch {
            if generation == c.generation { c.error = describe(error) }
        }
        if generation == c.generation { c.loading = false }
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
        diffs[diffKey(diff.tree, path: diff.path, at: at, ignoreWhitespace: false, context: nil)] = DiffDocument(diff)
    }

    public func take(_ page: GitCompare) {
        let c = compare(page.tree, against: page.against)
        c.take(page, cursor: 0, generation: c.generation)
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

    /// Stages every file of Changes, or unstages every staged one, reading the pages not loaded yet first.
    public func stageAll(_ tree: String, section: ChangeSection) async {
        let t = changes(tree)
        guard t.bulk == nil else { return }
        t.bulk = BulkStage(section: section, count: t.count(section))
        defer { t.bulk = nil }
        guard await complete(tree) else { return }
        let paths = t.paths(section)
        guard !paths.isEmpty else { return }
        await stage(tree, paths: paths, staged: section == .changes)
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
            t.error = describe(DaemonError.offline)
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
            t.error = describe(error)
            await refresh(tree)
            return false
        }
    }

    private func describe(_ error: any Error) -> String? { ErrorText.describe(error, mac: mac) }
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
    public var unpulled: Int?
    /// The desktop's setting: with nothing staged, a commit takes every change.
    public var commitAll: Bool
    /// The desktop's `git.force_with_lease`, while Push goes to the upstream.
    public var forceWithLease: Bool
    public var totals: GitTotals
    public var commits: [CommitRef]

    /// The upstream commits Push would replace, when HEAD rewrote what it pushed and the desktop allows a force push
    /// with lease; nil when Push needs no force or may not force.
    public var forceReplaces: Int? {
        guard forceWithLease, let unpushed, let unpulled, unpushed > 0, unpulled > 0 else { return nil }
        return unpulled
    }

    public init(_ s: GitStatus) {
        rev = s.rev
        branch = s.branch
        detached = s.detached ?? false
        upstream = s.upstream
        base = s.base
        ahead = s.ahead
        behind = s.behind
        unpushed = s.unpushed
        unpulled = s.unpulled
        commitAll = s.commitAll ?? true
        forceWithLease = s.forceWithLease ?? false
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
    /// A Stage All or Unstage All under way.
    public internal(set) var bulk: BulkStage?
    public var error: String?
    /// The Mac said the tree moved since the last read.
    public internal(set) var stale = false
    /// Bumped whenever `files` changes, so the list can keep its rows until it does.
    public private(set) var revision = 0
    /// Bumped by each optimistic change, so a read that started before it can't undo it.
    @ObservationIgnored var generation = 0
    /// Bumped by each read taken in from the first page.
    public private(set) var reads = 0
    @ObservationIgnored private var sectionPaths: (revision: Int, section: ChangeSection, paths: [String])?

    init(tree: String) { self.tree = tree }

    /// Takes in a page: the first replaces everything, a later one extends the list only while the rev holds.
    func take(_ page: GitStatus, cursor: Int, generation: Int) {
        guard cursor > 0 else { return take([page], generation: generation) }
        guard generation == self.generation, page.rev == head?.rev, cursor == files.count else { return }
        files.append(contentsOf: page.files)
        revision += 1
        nextCursor = page.nextCursor
    }

    /// Takes in pages read from the first on, replacing everything at once.
    func take(_ pages: [GitStatus], generation: Int) {
        guard generation == self.generation, let first = pages.first else { return }
        head = GitHead(first)
        files = pages.flatMap(\.files)
        stale = false
        reads += 1
        revision += 1
        nextCursor = pages.last?.nextCursor
    }

    func flip(_ paths: [String], staged: Bool) {
        generation += 1
        let before = (staged: files.count(where: \.staged), unstaged: files.count(where: \.unstaged))
        ChangeTree.flipStaged(&files, paths: paths, staged: staged)
        revision += 1
        head?.totals.staged += files.count(where: \.staged) - before.staged
        head?.totals.unstaged? += files.count(where: \.unstaged) - before.unstaged
    }

    /// A section's file count over the whole tree, not only the loaded pages.
    public func count(_ section: ChangeSection) -> Int {
        guard let totals = head?.totals else { return 0 }
        return section == .staged ? totals.staged : totals.unstaged ?? files.count(where: \.unstaged)
    }

    /// A section's loaded files, worked out once per change of `files` rather than on every redraw.
    public func paths(_ section: ChangeSection) -> [String] {
        if let kept = sectionPaths, kept.revision == revision, kept.section == section { return kept.paths }
        let paths = files.filter { section == .staged ? $0.staged : $0.unstaged }.map(\.path)
        sectionPaths = (revision, section, paths)
        return paths
    }

    /// Every changed file is loaded, so section-wide actions see them all.
    public var complete: Bool { nextCursor == nil }

    /// How many files are changed, known only from a read after the `reads`-th that the Mac hasn't said is outdated.
    public func files(readSince reads: Int) -> Int? {
        self.reads > reads && !stale ? head?.totals.files : nil
    }
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

    /// Takes a fresh first page; while the tips hold, the pages already loaded stay.
    func takeFirst(_ log: GitLog) {
        guard log.tips != tips || commits.isEmpty else { return }
        take(log, skip: 0)
    }
}

/// A section-wide stage and how many files it covers.
public struct BulkStage: Hashable, Sendable {
    public let section: ChangeSection
    public let count: Int

    public init(section: ChangeSection, count: Int) {
        self.section = section
        self.count = count
    }
}

/// What a tree committed since it left `against`, page by page; read-only.
@MainActor
@Observable
public final class TreeCompare {
    public let tree: String
    public let against: String
    public internal(set) var totals: CompareTotals?
    public internal(set) var files: [CommitFile] = []
    public internal(set) var nextCursor: Int?
    public internal(set) var loading = false
    public var error: String?
    /// `against` names no branch or commit.
    public internal(set) var missing = false
    /// Bumped by each first-page read, so a reply to an older read is dropped.
    @ObservationIgnored var generation = 0

    init(tree: String, against: String) {
        self.tree = tree
        self.against = against
    }

    /// The first page replaces everything; a later one extends the list where it ends. Both only from the newest read.
    func take(_ page: GitCompare, cursor: Int, generation: Int) {
        guard generation == self.generation else { return }
        if cursor == 0 {
            totals = page.totals
            files = page.files
            missing = false
            error = nil
        } else {
            guard cursor == files.count else { return }
            files.append(contentsOf: page.files)
        }
        nextCursor = page.nextCursor
    }
}

struct CompareKey: Hashable {
    var tree: String
    var against: String
}

struct DiffKey: Hashable {
    var tree: String
    var path: String
    var at: DiffAt
    var rev: String
    var ignoreWhitespace: Bool
    var context: Int?
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
