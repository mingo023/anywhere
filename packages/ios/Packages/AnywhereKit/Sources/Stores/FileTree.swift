import Wire

/// What the tree leaves out, as the desktop's Files settings `hide_ignored` and `hide_dotfiles` (defaults on, off).
public struct FileFilter: Hashable, Sendable, Codable {
    public var hideIgnored: Bool
    public var hideDotfiles: Bool

    public init(hideIgnored: Bool = true, hideDotfiles: Bool = false) {
        self.hideIgnored = hideIgnored
        self.hideDotfiles = hideDotfiles
    }

    public func shows(_ entry: FsEntry) -> Bool {
        !(hideIgnored && entry.ignored == true) && !(hideDotfiles && entry.name.hasPrefix("."))
    }
}

/// A line of the tree: a file, or a folder and whether it is open. `path` is relative to the tree and is the row's id.
public struct FileRow: Identifiable, Hashable, Sendable {
    public var id: String { path }
    public let path: String
    public let entry: FsEntry
    public let depth: Int
    public let open: Bool
    /// An open folder whose listing hasn't come yet.
    public let loading: Bool
    /// Why an open folder with nothing to show couldn't be listed.
    public let failure: String?

    public init(path: String, entry: FsEntry, depth: Int, open: Bool = false, loading: Bool = false, failure: String? = nil) {
        self.path = path
        self.entry = entry
        self.depth = depth
        self.open = open
        self.loading = loading
        self.failure = failure
    }
}

/// One tree's folders as listed, and which are open. Folders keep their listing when folded, so reopening one shows
/// it at once while a fresh one loads; folding keeps the open folders inside, as the desktop's Explorer does.
public struct FileTree: Equatable, Sendable {
    public private(set) var listings: [String: [FsEntry]] = [:]
    public private(set) var open: Set<String> = []
    /// Why folders couldn't be listed, until they are.
    public private(set) var failures: [String: String] = [:]

    public init(listings: [String: [FsEntry]] = [:], open: Set<String> = []) {
        self.listings = listings
        self.open = open
    }

    /// Takes a folder's listing; false when it already was this.
    @discardableResult
    public mutating func listed(_ dir: String, _ entries: [FsEntry]) -> Bool {
        let cleared = failures.removeValue(forKey: dir) != nil
        if listings[dir] == entries { return cleared }
        listings[dir] = entries
        return true
    }

    /// Records why a folder couldn't be listed; false when it already said so.
    @discardableResult
    public mutating func failed(_ dir: String, _ message: String) -> Bool {
        failures.updateValue(message, forKey: dir) != message
    }

    /// Forgets why a folder failed, as it is asked for again; false when it hadn't.
    @discardableResult
    public mutating func retrying(_ dir: String) -> Bool {
        failures.removeValue(forKey: dir) != nil
    }

    /// Forgets a folder that is gone, and everything listed inside it.
    public mutating func gone(_ dir: String) {
        let inside = { (key: String) in key == dir || key.hasPrefix(dir + "/") }
        listings = listings.filter { !inside($0.key) }
        open = open.filter { !inside($0) }
        failures = failures.filter { !inside($0.key) }
    }

    /// Opens or folds `dir`; true when it opened.
    @discardableResult
    public mutating func toggle(_ dir: String) -> Bool {
        if open.remove(dir) != nil { return false }
        open.insert(dir)
        return true
    }

    /// Opens `dir` and every folder holding it.
    public mutating func reveal(_ dir: String) {
        guard !dir.isEmpty else { return }
        open.insert(dir)
        open.formUnion(ancestors(dir))
    }

    /// The root and every open folder whose parents are open: what a refresh lists again.
    public var shown: [String] {
        [""] + open.filter { dir in ancestors(dir).allSatisfy(open.contains) }.sorted()
    }

    /// The tree top to bottom, descending into open folders only, without what `filter` hides.
    public func rows(_ filter: FileFilter) -> [FileRow] {
        var rows: [FileRow] = []
        push("", depth: 0, filter, into: &rows)
        return rows
    }

    private func push(_ dir: String, depth: Int, _ filter: FileFilter, into rows: inout [FileRow]) {
        for entry in listings[dir] ?? [] where filter.shows(entry) {
            let path = dir.isEmpty ? entry.name : dir + "/" + entry.name
            let isOpen = entry.dir && open.contains(path)
            let failure = isOpen && listings[path] == nil ? failures[path] : nil
            rows.append(
                FileRow(path: path, entry: entry, depth: depth, open: isOpen, loading: isOpen && listings[path] == nil && failure == nil, failure: failure))
            if isOpen { push(path, depth: depth + 1, filter, into: &rows) }
        }
    }

    private func ancestors(_ dir: String) -> [String] {
        var out: [String] = []
        var at = dir
        while let slash = at.lastIndex(of: "/") {
            at = String(at[..<slash])
            out.append(at)
        }
        return out
    }
}
