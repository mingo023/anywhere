import Wire

extension Sample {
    public static let tree = "/Users/mingo/.worktrees/anywhere/calm-otter"

    /// A status with `files` changed files across a deep tree, sorted by path, every status letter present.
    public static func gitStatus(files count: Int = 1200, tree: String = tree) -> GitStatus {
        let dirs = [
            "packages/desktop/crates/pocket/src/chat", "packages/desktop/crates/pocket/src/git_ui", "packages/desktop/crates/git/src",
            "packages/ios/Packages/AnywhereKit/Sources/Wire", "packages/ios/Packages/AnywhereKit/Sources/Stores", "packages/pocketd/internal/wsserver",
            "packages/pocketd/internal/proto/testdata/golden/server", "docs/mobile", "scripts", "",
        ]
        let exts = ["rs", "swift", "go", "json", "md", "ts", "toml", "yaml"]
        let letters: [FileStatus] = [.modified, .modified, .modified, .added, .deleted, .renamed, .untracked, .conflict]
        var files: [GitFile] = []
        for n in 0..<count {
            let dir = dirs[n % dirs.count]
            let name = "\(["view", "model", "store", "row", "lanes", "status", "golden"][n % 7])_\(n).\(exts[n % exts.count])"
            let path = dir.isEmpty ? name : "\(dir)/\(name)"
            let status = letters[n % letters.count]
            let staged = n % 3 == 0 && status != .untracked
            files.append(GitFile(
                path: path, oldPath: status == .renamed ? path.replacingOccurrences(of: "_\(n)", with: "_old\(n)") : nil, status: status,
                staged: staged, unstaged: !staged || n % 6 == 0, added: status == .untracked ? nil : (n * 7) % 120,
                removed: status == .untracked ? nil : (n * 3) % 40, binary: n % 97 == 0 ? true : nil))
        }
        files.sort { $0.path < $1.path }
        let added = files.compactMap(\.added).reduce(0, +)
        let removed = files.compactMap(\.removed).reduce(0, +)
        return GitStatus(
            tree: tree, rev: "r\(count)", branch: "calm-otter", upstream: "origin/calm-otter", base: "main", ahead: 4, behind: 1, unpushed: 2,
            totals: GitTotals(files: count, added: added, removed: removed, staged: files.filter(\.staged).count), files: files,
            commits: (1...4).map { CommitRef(sha: String(repeating: "\($0)", count: 7), subject: "Port the lanes, part \($0)") })
    }

    /// A diff of `lines` rows in hunks of 40, with word ranges on paired changes.
    public static func diff(lines count: Int = 6000, tree: String = tree) -> GitDiff {
        var rows: [DiffRow] = []
        var old = 1
        var new = 1
        while rows.count < count {
            rows.append(DiffRow(k: .hunk, t: "@@ -\(old),40 +\(new),41 @@ struct Lanes {"))
            for i in 0..<38 where rows.count < count {
                switch i % 9 {
                case 4:
                    rows.append(DiffRow(k: .deleted, o: old, t: "        let lane = open()  // \(old)", w: [[19, 25]]))
                    rows.append(DiffRow(k: .added, n: new, t: "        let lane = slots.firstIndex(of: sha) ?? open()  // \(new)", w: [[19, 53]]))
                    old += 1
                    new += 1
                case 7:
                    rows.append(DiffRow(k: .added, n: new, t: "        precondition(lane < slots.count, \"lane \\(lane) is past the slots\")"))
                    new += 1
                default:
                    rows.append(DiffRow(k: .context, o: old, n: new, t: "        slots[lane] = commit.parents.first // line \(old)"))
                    old += 1
                    new += 1
                }
            }
            old += 120
            new += 121
        }
        let added = rows.filter { $0.k == .added }.count
        let removed = rows.filter { $0.k == .deleted }.count
        return GitDiff(tree: tree, path: "Sources/DiffModel/Lanes.swift", language: "swift", added: added, removed: removed, lines: rows)
    }
}
