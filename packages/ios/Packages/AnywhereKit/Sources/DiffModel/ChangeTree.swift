import Wire

/// The two lists of the Changes screen.
public enum ChangeSection: String, Hashable, Sendable, CaseIterable {
    case staged, changes

    public var title: String { self == .staged ? "Staged" : "Changes" }
}

/// One uniform-height row of the Changes list (pocket/git_ui/changes.rs `Row`), keyed so it keeps its identity across refreshes.
public enum ChangeRow: Hashable, Sendable, Identifiable {
    /// A section's header, with every path under it for the section's stage-all.
    case header(ChangeSection, paths: [String])
    /// A folder of the tree view; `label` may span a chain of single-folder folders.
    case folder(ChangeSection, key: String, label: String, depth: Int, open: Bool)
    case file(ChangeSection, GitFile, depth: Int)

    public var id: String {
        switch self {
        case .header(let s, _): s.rawValue
        case .folder(_, let key, _, _, _): key
        case .file(let s, let f, _): "\(s.rawValue):\(f.path)"
        }
    }
}

public enum ChangeTree {
    /// A header per non-empty section, then its files flat or as a folded tree.
    public static func rows(_ files: [GitFile], tree asTree: Bool, folded: Set<String>) -> [ChangeRow] {
        var rows: [ChangeRow] = []
        for s in ChangeSection.allCases {
            let picked = files.filter { s == .staged ? $0.staged : $0.unstaged }
            if picked.isEmpty { continue }
            rows.append(.header(s, paths: picked.map(\.path)))
            if folded.contains(s.rawValue) { continue }
            if asTree {
                tree(picked[...], base: "", depth: 0, section: s, folded: folded, into: &rows)
            } else {
                rows.append(contentsOf: picked.map { .file(s, $0, depth: 0) })
            }
        }
        return rows
    }

    /// Folders first, each chain of single-folder folders merged into one row, then the files directly under `base`.
    static func tree(_ files: ArraySlice<GitFile>, base: String, depth: Int, section: ChangeSection, folded: Set<String>, into out: inout [ChangeRow]) {
        var dirs: [Substring: [GitFile]] = [:]
        var order: [Substring] = []
        var leaves: [GitFile] = []
        for f in files {
            let rest = f.path.dropFirst(base.count)
            if let slash = rest.firstIndex(of: "/") {
                let dir = rest[..<slash]
                if dirs[dir] == nil { order.append(dir) }
                dirs[dir, default: []].append(f)
            } else {
                leaves.append(f)
            }
        }
        for dir in order.sorted() {
            let sub = dirs[dir]!
            var path = base + dir + "/"
            while let next = commonDir(sub, base: path) { path += next + "/" }
            let key = "\(section.rawValue):\(path)"
            let open = !folded.contains(key)
            out.append(.folder(section, key: key, label: String(path.dropFirst(base.count).dropLast()), depth: depth, open: open))
            if open { tree(sub[...], base: path, depth: depth + 1, section: section, folded: folded, into: &out) }
        }
        out.append(contentsOf: leaves.map { .file(section, $0, depth: depth) })
    }

    /// The folder every file continues into below `base`, if they share one.
    static func commonDir(_ files: [GitFile], base: String) -> Substring? {
        var shared: Substring?
        for f in files {
            let rest = f.path.dropFirst(base.count)
            guard let slash = rest.firstIndex(of: "/") else { return nil }
            let dir = rest[..<slash]
            if let s = shared, s != dir { return nil }
            shared = dir
        }
        return shared
    }

    /// `git add -A` and `git reset` leave each picked path wholly staged or wholly unstaged, so the rows flip at once.
    public static func flipStaged(_ files: inout [GitFile], paths: [String], staged: Bool) {
        let picked = Set(paths)
        for i in files.indices where picked.contains(files[i].path) {
            files[i].staged = staged
            files[i].unstaged = !staged
        }
    }

    public static func toggleFold(_ folded: inout Set<String>, _ key: String) {
        if folded.remove(key) == nil { folded.insert(key) }
    }
}

/// How a commit is made from the composer.
public enum CommitKind: Hashable, Sendable {
    case commit, push, amend
}

/// What the composer's button does now (changes.rs `commit_step`).
public enum CommitStep: Hashable, Sendable {
    case skip
    case askMessage
    /// Nothing is staged and the settings say to commit only what is.
    case stageFirst
    /// `all` stages every change first, because none is staged.
    case run(all: Bool, busy: String)
}

public enum Composer {
    /// Commits the staged files, or with `commitAll` every change when none is staged. Amending keeps the last
    /// message unless a new one is typed.
    public static func step(files: [GitFile], kind: CommitKind, message: String, busy: Bool, commitAll: Bool) -> CommitStep {
        let amend = kind == .amend
        if busy || (!amend && files.isEmpty) { return .skip }
        let noneStaged = !files.contains(where: \.staged)
        if !amend, noneStaged, !commitAll { return .stageFirst }
        if !amend, message.isEmpty { return .askMessage }
        return .run(all: !amend && noneStaged, busy: amend ? "Amending…" : "Committing…")
    }

    public static func label(files: [GitFile], busy: String?, commitAll: Bool) -> String {
        if let busy { return busy }
        return commitAll && !files.isEmpty && !files.contains(where: \.staged) ? "Commit All" : "Commit"
    }

    public static func ready(files: [GitFile], message: String, busy: Bool, commitAll: Bool) -> Bool {
        !files.isEmpty && !message.allSatisfy(\.isWhitespace) && !busy && (commitAll || files.contains(where: \.staged))
    }
}
