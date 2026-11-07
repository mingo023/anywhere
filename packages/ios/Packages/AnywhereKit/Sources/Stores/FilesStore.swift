import Client
import Domain
import Foundation
import Observation
import Wire

/// What the Files screens hand to a chat's draft: `@path`, or a quote of lines. One for no agent goes to the next chat
/// on `tree` to come on screen.
public struct ChatAddition: Hashable, Sendable {
    public let agentId: String?
    public let tree: String
    public let text: String
}

/// File trees, previews, go-to-file and image uploads for one Mac's trees.
///
/// For the chat composer: `search(tree:query:limit:)` answers @ mentions, `upload(_:name:)` sends an image and returns
/// the path on the Mac to put in the prompt (progress in `uploads`), and `takeAdditions(for:in:)` hands over, in order,
/// the files and lines the Files screens queued for that chat.
@MainActor
@Observable
public final class FilesStore: HostStore {
    @ObservationIgnored let connection: DaemonConnection?
    @ObservationIgnored private var trees: [String: FileTreeModel] = [:]
    @ObservationIgnored private var previews: [String: FilePreview] = [:]
    /// Preview keys, least recently opened first.
    @ObservationIgnored private var recent: [String] = []
    /// The Mac's name, as errors say it; its hostname once it has said hello.
    @ObservationIgnored public var mac = FilesStore.unnamed

    /// What every tree hides.
    public var filter = FileFilter() {
        didSet { for tree in trees.values { tree.filter = filter } }
    }
    /// Upload progress 0...1 by upload id, while each runs.
    public private(set) var uploads: [String: Double] = [:]
    /// What waits for a chat's draft, oldest first.
    public private(set) var additions: [ChatAddition] = []

    /// The Mac before it says its name.
    public nonisolated static let unnamed = "the Mac"

    /// How many previews stay cached; images can be 20 MB each.
    static let previewsKept = 8

    public init(connection: DaemonConnection? = nil, mac: String = FilesStore.unnamed) {
        self.connection = connection
        self.mac = mac
    }

    // MARK: Tree

    /// The tree's model, made on first ask without loading; `open` loads it.
    public func tree(_ tree: String) -> FileTreeModel {
        if let model = trees[tree] { return model }
        let model = FileTreeModel(tree: tree, filter: filter)
        trees[tree] = model
        return model
    }

    /// Lists the root and every open folder again; the first open lists the root.
    public func open(_ model: FileTreeModel) {
        for dir in model.state.shown { list(model.tree, dir) }
    }

    /// Opens a folder and those holding it, as a route to a folder asks.
    public func reveal(_ model: FileTreeModel, _ dir: String) {
        model.reveal(dir)
    }

    /// Opens or folds a folder; an opened one shows what it held last time while it is listed again.
    public func toggle(_ model: FileTreeModel, _ dir: String) {
        if model.toggle(dir) { list(model.tree, dir) }
    }

    /// Lists a folder that failed again.
    public func retry(_ model: FileTreeModel, _ dir: String) {
        list(model.tree, dir)
    }

    private func list(_ tree: String, _ dir: String) {
        trees[tree]?.asked(dir)
        Task {
            do {
                _ = apply(try await request(.fsList(.init(tree: tree, dir: dir))))
            } catch {
                trees[tree]?.failed(dir, describe(error), refused: Self.refused(error))
            }
        }
    }

    // MARK: Preview

    /// The file's preview, cached; `load` fetches it.
    public func preview(tree: String, path: String) -> FilePreview {
        let key = Self.key(tree, path)
        recent.removeAll { $0 == key }
        recent.append(key)
        if let preview = previews[key] { return preview }
        let preview = FilePreview(tree: tree, path: path)
        previews[key] = preview
        while recent.count > Self.previewsKept { previews[recent.removeFirst()] = nil }
        return preview
    }

    /// Reads the file again; what shows changes only if the file did.
    public func load(_ preview: FilePreview) {
        let (tree, path) = (preview.tree, preview.path)
        preview.reading()
        Task {
            do {
                let reply = try await request(.fsRead(.init(tree: tree, path: path)))
                // fs.read can't tell a folder from a file it can't read; a folder lists.
                if case .fsRead(let read) = reply, read.kind == .unreadable, read.mtime == 0,
                    case .fsList? = try? await request(.fsList(.init(tree: tree, dir: path)))
                {
                    preview.folder()
                } else {
                    _ = apply(reply)
                }
            } catch {
                preview.failed(describe(error))
            }
        }
    }

    /// The bytes of an image in the tree, as a markdown file there shows it; nil for anything else.
    public func image(tree: String, path: String) async -> Data? {
        guard case .fsRead(let read)? = try? await request(.fsRead(.init(tree: tree, path: path))), read.kind == .image else { return nil }
        return read.data
    }

    // MARK: Search

    /// Go-to-file over a tree, best first, as the composer's @ mentions and the Files search ask.
    public func search(tree: String, query: String, limit: Int = Limits.fsSearchLimit) async throws -> [String] {
        guard case .fsSearch(let found) = try await request(.fsSearch(.init(tree: tree, query: query, limit: limit))) else { return [] }
        return found.paths
    }

    /// A debounced search for the go-to-file sheet.
    public func makeSearch(tree: String) -> FileSearch {
        FileSearch(tree: tree, mac: mac) { [weak self] query in
            guard let self else { return [] }
            return try await self.search(tree: tree, query: query)
        }
    }

    // MARK: Attachments

    /// Sends an image in chunks and returns where it landed on the Mac, under its attachments folder.
    public func upload(_ data: Data, name: String, id: String = Attachments.newId()) async throws -> String {
        try Attachments.check(data, name: name)
        let count = Attachments.chunkCount(data.count)
        uploads[id] = 0
        defer { uploads[id] = nil }
        for index in 0..<count {
            let reply = try await request(.attachPut(Attachments.chunk(index, of: data, uploadId: id, name: name)))
            uploads[id] = Double(index + 1) / Double(count)
            if case .attachDone(let done) = reply { return done.path }
        }
        throw DaemonError.encoding("The Mac didn't say where the image landed")
    }

    // MARK: Chat drafts

    /// Queues `@path` for a chat's draft; with no agent, for the next chat on `tree` to come on screen.
    public func mention(tree: String, path: String, for agentId: String? = nil) {
        add(ChatAddition(agentId: agentId, tree: tree, text: "@" + path))
    }

    /// Queues a quote of lines (`CodeQuote.payload`) for a chat's draft.
    public func quote(_ text: String, tree: String, for agentId: String) {
        add(ChatAddition(agentId: agentId, tree: tree, text: text))
    }

    private func add(_ addition: ChatAddition) {
        if !additions.contains(addition) { additions.append(addition) }
    }

    /// What waits for the chat of `agentId` on `tree`, oldest first, handed over once. Put it in the draft with
    /// `draft(_:adding:)`.
    public func takeAdditions(for agentId: String, in tree: String) -> [String] {
        let taken = additions.filter { $0.agentId == agentId || ($0.agentId == nil && $0.tree == tree) }
        guard !taken.isEmpty else { return [] }
        additions.removeAll { taken.contains($0) }
        return taken.map(\.text)
    }

    /// `text` with `pieces` added at its end: a mention after a space, a quote on lines of its own.
    public nonisolated static func draft(_ text: String, adding pieces: [String]) -> String {
        pieces.reduce(text) { text, piece in
            if piece.contains("\n") {
                let gap = text.isEmpty || text.hasSuffix("\n\n") ? "" : text.hasSuffix("\n") ? "\n" : "\n\n"
                return text + gap + piece + "\n"
            }
            return text + (text.isEmpty || text.last!.isWhitespace ? "" : " ") + piece + " "
        }
    }

    // MARK: HostStore

    public func apply(_ message: ServerMessage) -> Bool {
        switch message {
        case .fsList(let list):
            return trees[list.tree]?.listed(list.dir, list.entries) ?? false
        case .fsRead(let read):
            return previews[Self.key(read.tree, read.path)]?.loaded(read) ?? false
        case .gitChanged(let changed):
            // A watched tree changed on disk: its letters, listings and files may have. Images stay, as they are large.
            if let model = trees[changed.tree] { open(model) }
            for preview in previews.values where preview.tree == changed.tree && !preview.isImage { load(preview) }
            return false
        default:
            return false
        }
    }

    public func connected(_ hello: HelloOk) {
        mac = hello.hostname
        for model in trees.values where model.hasFailures || !model.waiting.isEmpty { open(model) }
        for preview in previews.values where preview.lastReadFailed { load(preview) }
    }

    private func request(_ message: ClientMessage) async throws -> ServerMessage {
        guard let connection else { throw DaemonError.offline }
        return try await connection.request(message)
    }

    private nonisolated static func refused(_ error: any Error) -> Bool {
        if case DaemonError.server = error { true } else { false }
    }

    private static func key(_ tree: String, _ path: String) -> String { tree + "\u{0}" + path }

    /// What a failed request says to the reader, naming the Mac; nil for one that was cancelled.
    public func describe(_ error: any Error) -> String? { Self.describe(error, mac: mac) }

    public nonisolated static func describe(_ error: any Error, mac: String) -> String? {
        switch error {
        case AttachmentError.unsupportedType: "Only PNG, JPEG, GIF, WebP and HEIC images can be attached"
        case AttachmentError.tooLarge: "Images over 20 MB can't be attached"
        case AttachmentError.empty: "That image is empty"
        default: ErrorText.describe(error, mac: mac)
        }
    }
}

/// One tree as the Files screen shows it: its rows, and what is loading or failed.
@MainActor
@Observable
public final class FileTreeModel {
    public let tree: String
    public private(set) var rows: [FileRow] = []
    public private(set) var state = FileTree()
    /// Why the root couldn't be listed.
    public private(set) var failure: String?
    /// Folders asked for and not answered.
    public private(set) var waiting: Set<String> = []
    /// The root has been listed once.
    public var loaded: Bool { state.listings[""] != nil }
    /// The root or a folder couldn't be listed, so a reconnect lists them again.
    var hasFailures: Bool { failure != nil || !state.failures.isEmpty }

    var filter: FileFilter {
        didSet { if filter != oldValue { rebuild() } }
    }

    public init(tree: String, filter: FileFilter = FileFilter()) {
        self.tree = tree
        self.filter = filter
    }

    func asked(_ dir: String) {
        waiting.insert(dir)
        if state.retrying(dir) { rebuild() }
    }

    @discardableResult
    func listed(_ dir: String, _ entries: [FsEntry]) -> Bool {
        waiting.remove(dir)
        if dir.isEmpty { failure = nil }
        guard state.listed(dir, entries) else { return false }
        rebuild()
        return true
    }

    func reveal(_ dir: String) {
        state.reveal(dir)
        rebuild()
    }

    func toggle(_ dir: String) -> Bool {
        let opened = state.toggle(dir)
        rebuild()
        return opened
    }

    /// The root says why it couldn't be listed. A folder the Mac refused (gone, or outside the tree) folds and forgets
    /// what it held; one that failed for the connection keeps it and says why. A cancelled ask (no message) still waits.
    func failed(_ dir: String, _ message: String?, refused: Bool) {
        guard let message else { return }
        waiting.remove(dir)
        if dir.isEmpty {
            failure = message
        } else if refused {
            state.gone(dir)
            rebuild()
        } else if state.failed(dir, message) {
            rebuild()
        }
    }

    private func rebuild() { rows = state.rows(filter) }
}

/// One file's preview.
@MainActor
@Observable
public final class FilePreview {
    public enum Phase: Equatable, Sendable {
        case loading
        case loaded(FsRead)
        case failed(String)
        /// The path is a folder.
        case folder
    }

    public let tree: String
    public let path: String
    public private(set) var phase = Phase.loading
    /// Why a reload over what shows failed.
    public private(set) var notice: String?

    public init(tree: String, path: String) {
        self.tree = tree
        self.path = path
    }

    /// The last read failed, so a reconnect reads it again.
    var lastReadFailed: Bool {
        if case .failed = phase { return true }
        return notice != nil
    }

    var isImage: Bool {
        if case .loaded(let read) = phase { read.kind == .image } else { false }
    }

    /// Takes a read; false when it shows what already showed.
    @discardableResult
    func loaded(_ read: FsRead) -> Bool {
        if notice != nil { notice = nil }
        if case .loaded(let shown) = phase, shown.mtime == read.mtime, shown.size == read.size, shown == read { return false }
        phase = .loaded(read)
        return true
    }

    /// A failed reload keeps what showed and says why over it. A cancelled one (no message) says nothing.
    func failed(_ message: String?) {
        guard let message else { return }
        if case .loaded = phase {
            notice = message
        } else {
            phase = .failed(message)
        }
    }

    func folder() { phase = .folder }

    /// A preview that failed shows it loads again.
    func reading() {
        if case .failed = phase { phase = .loading }
    }
}

/// The go-to-file sheet's search: what was found narrows at once as the query grows, and the Mac is asked once the
/// typing pauses; only the answer to the latest query lands.
@MainActor
@Observable
public final class FileSearch {
    public let tree: String
    public var query = "" {
        didSet { if query != oldValue { changed() } }
    }
    public private(set) var results: [String] = []
    public private(set) var searching = false
    public private(set) var failure: String?

    @ObservationIgnored private let fetch: @MainActor (String) async throws -> [String]
    @ObservationIgnored private let delay: Duration
    @ObservationIgnored private let mac: String
    @ObservationIgnored private var task: Task<Void, Never>?
    @ObservationIgnored private var asked = 0

    /// `query` and `results` start the sheet where a previous one left off. `mac` names the Mac in errors.
    public init(
        tree: String, query: String = "", results: [String] = [], delay: Duration = .milliseconds(120), mac: String = FilesStore.unnamed,
        fetch: @escaping @MainActor (String) async throws -> [String]
    ) {
        self.tree = tree
        self.mac = mac
        self.query = query
        self.results = results
        self.delay = delay
        self.fetch = fetch
    }

    /// Asks for the empty query's list (changed files first) at once.
    public func start() { ask(after: .zero) }

    private func changed() {
        if !query.isEmpty, !results.isEmpty {
            results = FuzzyIndex(results).search(query, limit: results.count)
        }
        ask(after: delay)
    }

    private func ask(after wait: Duration) {
        task?.cancel()
        asked += 1
        let (n, q, fetch) = (asked, query, fetch)
        searching = true
        task = Task { [weak self] in
            if wait > .zero { try? await Task.sleep(for: wait) }
            guard !Task.isCancelled else { return }
            do {
                let found = try await fetch(q)
                self?.answer(n, .success(found))
            } catch {
                self?.answer(n, .failure(error))
            }
        }
    }

    func answer(_ n: Int, _ result: Result<[String], any Error>) {
        guard n == asked else { return }
        searching = false
        switch result {
        case .success(let found):
            results = found
            failure = nil
        case .failure(let error):
            failure = FilesStore.describe(error, mac: mac)
        }
    }
}
