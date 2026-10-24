import Wire

extension Sample {
    /// Forty hex digits from a number, so shas are stable and distinct.
    static func sha(_ n: Int) -> String {
        let hex = String(UInt64(truncatingIfNeeded: n) &* 0x9E37_79B9_7F4A_7C15, radix: 16)
        return String((hex + String(repeating: "0", count: 40)).prefix(40))
    }

    /// History newest first: a mainline that merges a short side branch every eight commits, with HEAD, its upstream
    /// two behind and the base ten behind.
    public static func log(commits count: Int = 50, tree: String = tree) -> GitLog {
        let subjects = [
            "Port the lanes to Swift", "Page the history by fifty", "Fold single-folder chains", "Cut long lines at 1024 chars",
            "Stamp the status with mtimes", "Mark changed words on paired lines", "Serve git.v1 off the read loop", "Merge branch 'side'",
        ]
        let authors = ["Minh Ngo", "Ada", "Grace Hopper"]
        var out: [LogCommit] = []
        var n = 0
        var main = 0
        while out.count < count {
            let at = 1_792_832_400 - out.count * 1_800
            if main % 8 == 7, out.count + 3 <= count {
                let side = 100_000 + n
                out.append(LogCommit(sha: sha(n), parents: [sha(n + 3), sha(side)], author: authors[0], at: at, subject: "Merge branch 'side-\(main)'"))
                out.append(LogCommit(sha: sha(side), parents: [sha(side + 1)], author: authors[1], at: at - 600, subject: "Try the side idea"))
                out.append(LogCommit(sha: sha(side + 1), parents: [sha(n + 3)], author: authors[1], at: at - 1_200, subject: "Start the side idea"))
                n += 3
            } else {
                out.append(
                    LogCommit(
                        sha: sha(n), parents: out.count + 1 < count ? [sha(n + 1)] : [], author: authors[main % authors.count], at: at,
                        subject: subjects[main % subjects.count]))
                n += 1
            }
            main += 1
        }
        if let last = out.indices.last { out[last].parents = [] }
        let tips = LogTips(head: out.first?.sha, branch: "calm-otter", upstream: out.count > 2 ? out[2].sha : nil, base: out.count > 10 ? out[10].sha : nil)
        return GitLog(tips: tips, commits: Array(out.prefix(count)), more: true)
    }

    public static let commit = CommitDetail(
        sha: sha(0), subject: "Port the lanes to Swift", body: "Lanes::push stays stateful across pages, so page N+1 continues page N.\n\nCloses #121.",
        author: "Minh Ngo", at: 1_792_832_400,
        files: [
            CommitFile(path: "packages/ios/Packages/AnywhereKit/Sources/DiffModel/Lanes.swift", status: .added, added: 96, removed: 0),
            CommitFile(path: "packages/ios/Packages/AnywhereKit/Tests/DiffModelTests/DiffModelTests.swift", status: .modified, added: 74, removed: 2),
            CommitFile(path: "docs/mobile/graph.md", oldPath: "docs/graph.md", status: .renamed, added: 3, removed: 1),
            CommitFile(path: "packages/ios/App/Assets/graph.png", status: .added),
            CommitFile(path: "packages/desktop/crates/git/src/old_graph.rs", status: .deleted, added: 0, removed: 140),
        ])

    /// A short working diff with a paired change, as a git.diff reply.
    public static let smallDiff = GitDiff(
        tree: tree, path: "packages/ios/Packages/AnywhereKit/Sources/DiffModel/Lanes.swift", language: "swift", added: 3, removed: 2,
        lines: [
            DiffRow(k: .hunk, t: "@@ -30,9 +30,10 @@ public struct Lanes: Sendable {"),
            DiffRow(k: .context, o: 30, n: 30, t: "    /// Lays out the next commit, newest first."),
            DiffRow(k: .context, o: 31, n: 31, t: "    public mutating func push(_ sha: String, parents: [String]) -> GraphRow {"),
            DiffRow(k: .deleted, o: 32, t: "        let input = std::mem::take(&mut self.open);", w: [[20, 47]]),
            DiffRow(k: .deleted, o: 33, t: "        let column = input.iter().position(|l| l.sha == sha);", w: [[29, 63]]),
            DiffRow(k: .added, n: 32, t: "        let input = open", w: [[20, 24]]),
            DiffRow(k: .added, n: 33, t: "        let column = input.firstIndex { $0.sha == sha } ?? input.count", w: [[29, 72]]),
            DiffRow(k: .added, n: 34, t: "        var output: [Lane] = []"),
            DiffRow(k: .context, o: 34, n: 35, t: "        var placed = false"),
            DiffRow(k: .context, o: 35, n: 36, t: "        for lane in input {"),
            DiffRow(k: .hunk, t: "@@ -61,4 +62,4 @@ public struct Lanes: Sendable {"),
            DiffRow(k: .context, o: 61, n: 62, t: "    private mutating func nextColor() -> LaneColor {"),
            DiffRow(k: .deleted, o: 62, t: "        next += 1; return .lane(next - 1)"),
            DiffRow(k: .added, n: 63, t: "        defer { next += 1 }"),
            DiffRow(k: .context, o: 63, n: 64, t: "    }"),
        ])
}
