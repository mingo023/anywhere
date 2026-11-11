import Foundation

/// The desktop sidebar's rules, as the phone's home list uses them: which tree a folder belongs to
/// (`crates/project`), how a tree's sessions sort and roll up (`pocket/src/status.rs`), and how rows are named and searched
/// (`pocket/src/sidebar*.rs`).
public enum Sidebar {
    /// `cwd` is `root` or inside it. A root saved with a trailing slash holds only itself.
    public static func under(_ cwd: String, _ root: String) -> Bool {
        cwd == root || (cwd.hasPrefix(root) && cwd.dropFirst(root.count).first == "/")
    }

    /// The project owning `cwd`: the one whose folder or worktree holds it most closely; a later project wins a tie.
    /// `trees` are each project's worktree paths.
    public static func projectOf(_ cwd: String, projects: [String], trees: [String: [String]]) -> String? {
        var best: (length: Int, project: String)?
        for p in projects {
            let roots = [p] + (trees[p] ?? [])
            guard let reach = roots.filter({ under(cwd, $0) }).map(\.count).max() else { continue }
            if best == nil || reach >= best!.length { best = (reach, p) }
        }
        return best?.project
    }

    /// The listed worktree holding `cwd` most closely.
    public static func worktreeOf(_ cwd: String, trees: [String]) -> String? {
        trees.filter { under(cwd, $0) }.max { $0.count < $1.count }
    }

    /// The tree a folder belongs to: the worktree holding it, else the main tree of the project it is in.
    /// `mains` maps a project to its main worktree's path.
    public static func treeOf(_ cwd: String, projects: [String], trees: [String: [String]], mains: [String: String]) -> String? {
        if let tree = worktreeOf(cwd, trees: trees.values.flatMap(\.self)) { return tree }
        return projectOf(cwd, projects: projects, trees: trees).map { mains[$0] ?? $0 }
    }

    // MARK: Sessions

    public enum SessionSort: String, CaseIterable, Hashable, Sendable {
        case newest, status, activity, name
    }

    /// What sorting and rolling up need from a session.
    public struct Card: Hashable, Sendable, Identifiable {
        public var id: String
        public var title: String
        public var status: Status
        /// Unix milliseconds.
        public var createdAt: Int
        /// Unix milliseconds of its last status step.
        public var updatedAt: Int
        public var pinned: Bool

        public init(id: String, title: String, status: Status, createdAt: Int, updatedAt: Int, pinned: Bool = false) {
            self.id = id
            self.title = title
            self.status = status
            self.createdAt = createdAt
            self.updatedAt = updatedAt
            self.pinned = pinned
        }
    }

    /// A tree's sessions: pinned ones first, each part in `by`'s order (`status::sort`). Ties keep their order.
    public static func sorted(_ cards: [Card], by: SessionSort = .newest) -> [Card] {
        let key: (Card, Card) -> Bool
        switch by {
        case .newest: key = { $0.createdAt > $1.createdAt }
        case .status: key = { $0.status != $1.status ? $0.status < $1.status : $0.createdAt > $1.createdAt }
        case .activity: key = { $0.updatedAt > $1.updatedAt }
        case .name:
            key = { a, b in
                let (x, y) = (a.title.lowercased(), b.title.lowercased())
                return x != y ? x < y : a.createdAt > b.createdAt
            }
        }
        let indexed = cards.enumerated().map { ($0.offset, $0.element) }
        let stable = indexed.sorted { a, b in
            if a.1.pinned != b.1.pinned { return a.1.pinned }
            if key(a.1, b.1) { return true }
            if key(b.1, a.1) { return false }
            return a.0 < b.0
        }
        return stable.map(\.1)
    }

    /// 0 Needs you, 1 Failed and Done, 2 Working, 3 Idle today, 4 Idle before.
    public static func section(_ status: Status, today: Bool) -> Int {
        switch status {
        case .needsYou: 0
        case .failed, .done: 1
        case .working: 2
        case .idle: today ? 3 : 4
        }
    }

    /// A group's most urgent status and how many of its sessions have exactly that status, so the count matches the word; nil when all are idle.
    public static func rollUp(_ statuses: [Status]) -> (status: Status, count: Int)? {
        guard let top = statuses.min(), top != .idle else { return nil }
        return (top, statuses.filter { $0 == top }.count)
    }

    /// How many sessions have each status but Idle, most urgent first.
    public static func breakdown(_ statuses: [Status]) -> [(status: Status, count: Int)] {
        Status.allCases.filter { $0 != .idle }.compactMap { s in
            let n = statuses.filter { $0 == s }.count
            return n > 0 ? (s, n) : nil
        }
    }

    /// The sessions whose title holds `query`, in any case, in the order given.
    public static func matching(_ cards: [Card], _ query: String) -> [Card] {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        guard !q.isEmpty else { return cards }
        return cards.filter { $0.title.lowercased().contains(q) }
    }

    /// The keys of `trees` (key, label) a project named `name` lists under `query`: all when the name matches,
    /// else those whose label does; nil when nothing matches.
    public static func shown(_ query: String, name: String, trees: [(key: String, label: String)]) -> [String]? {
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        let all = q.isEmpty || name.lowercased().contains(q)
        let keys = trees.filter { all || $0.label.lowercased().contains(q) }.map(\.key)
        return all || !keys.isEmpty ? keys : nil
    }

    // MARK: Names

    /// A worktree's place: its branch, or its folder when detached.
    public static func treePlace(path: String, branch: String) -> String {
        branch.isEmpty || branch == "detached" ? basename(path) : branch
    }

    /// A worktree row's label and its place under it: the name it was given over its place, else the place twice.
    public static func treeLabel(path: String, branch: String, given: String?) -> (label: String, place: String) {
        let place = treePlace(path: path, branch: branch)
        return (given.flatMap { $0.isEmpty ? nil : $0 } ?? place, place)
    }

    /// The project's own Local: the name it was given, else "Local".
    public static func ownLocalName(given: String?) -> String {
        given.flatMap { $0.isEmpty ? nil : $0 } ?? "Local"
    }

    /// "Local 1", "Local 2", …: the first number none of `taken` uses.
    public static func nextLocalName(_ taken: [String]) -> String {
        var n = 1
        while taken.contains("Local \(n)") { n += 1 }
        return "Local \(n)"
    }

    /// A fresh Local id: pocketd takes letters, digits, `-` and `_`, and never a leading `/`.
    public static func newLocalId(now: Date = Date()) -> String {
        "local-" + String(UInt64(now.timeIntervalSince1970 * 1_000_000_000), radix: 16)
    }

    /// "claude-haiku-4-5-20251001" reads as "Haiku 4.5", "haiku" as "Haiku", "openai-codex/gpt-5.4" as "GPT 5.4",
    /// "opencode/big-pickle" as "Big Pickle" and "us.anthropic.claude-opus-5-5" as "Opus 5.5": the id without who serves
    /// it, its date or "claude-", numbers joined by a dot. Claude's "[1m]" for its 1M-context window stays as written.
    /// The desktop's `agents::model_name` is the reference: this follows it, never the other way round.
    public static func modelName(_ model: String) -> String {
        let isNumber = { (word: Substring) in !word.isEmpty && word.allSatisfy { $0.isASCII && $0.isNumber } }
        var id = model.split(separator: "/", omittingEmptySubsequences: false).last ?? ""
        // Hermes writes "provider:model"; Bedrock ends an id in ":0".
        if let colon = id.lastIndex(of: ":"), !isNumber(id[id.index(after: colon)...]) { id = id[id.index(after: colon)...] }
        let window = id.firstIndex(of: "[").map { id[$0...] } ?? ""
        id = id.dropLast(window.count)
        // Bedrock leads with where and who, as "us.anthropic."; the dot of a version, as "v3.2", has no two letters after it.
        let named = { (rest: Substring) in rest.count > 1 && rest.prefix(2).allSatisfy { $0.isASCII && $0.isLetter } }
        if let dot = id.indices.last(where: { id[$0] == "." && named(id[id.index(after: $0)...]) }) { id = id[id.index(after: dot)...] }
        if id.hasPrefix("claude-") { id = id.dropFirst(7) }
        var name = ""
        var afterNumber = false
        for word in id.split(separator: "-") where !(word.count >= 8 && isNumber(word)) {
            if !name.isEmpty { name += afterNumber && isNumber(word) ? "." : " " }
            // A word without a vowel is initials, as "gpt".
            name += word.contains(where: "aeiouy".contains) ? word.prefix(1).uppercased() + word.dropFirst() : word.uppercased()
            afterNumber = isNumber(word)
        }
        return name + window
    }

    static func basename(_ path: String) -> String {
        path.split(separator: "/").last.map(String.init) ?? path
    }
}
