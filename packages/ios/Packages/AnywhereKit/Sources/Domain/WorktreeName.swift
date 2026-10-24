import Foundation

/// The new-worktree rules of the desktop's new session page (`pocket/src/modals/new_session.rs`, `modals/form.rs`).
public enum WorktreeName {
    public static let takenProblem = "A worktree or branch with this name already exists"
    public static let invalidProblem = "Use letters, digits, - _ or ."

    /// Case-insensitive because APFS is; a branch `a/b` owns the name `a`.
    public static func isTaken(_ name: String, _ taken: Set<String>) -> Bool {
        taken.contains { t in
            let first = t.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: false).first.map(String.init) ?? t
            return first.caseInsensitiveCompare(name) == .orderedSame
        }
    }

    /// Why `name` can't name a new worktree and its branch, if it can't.
    public static func problem(_ name: String, taken: Set<String>) -> String? {
        let safe = name.unicodeScalars.allSatisfy { c in
            c.isASCII && (CharacterSet.alphanumerics.contains(c) || "-_.".unicodeScalars.contains(c))
        }
        let valid = !name.isEmpty && safe && !name.hasPrefix("-") && !name.hasPrefix(".") && !name.hasSuffix(".")
            && !name.hasSuffix(".lock") && !name.contains("..") && name != "HEAD"
        if isTaken(name, taken) { return takenProblem }
        return valid ? nil : invalidProblem
    }

    /// Branches count, and every remote's too, without the remote's name, as pocketd's `worktree.Taken`: closing a worktree
    /// deletes its folder and local branch, but its pushed branch lives on.
    public static func taken(local: [String], remote: [String], folders: [String] = []) -> Set<String> {
        Set(local + remote.map { String($0.split(separator: "/", maxSplits: 1).last ?? Substring($0)) } + folders)
    }

    /// The remote branches a worktree can open: pocketd's `PrepareBranch` fetches from origin only.
    public static func openable(remote: [String]) -> [String] {
        remote.filter { $0.hasPrefix("origin/") }
    }

    /// "origin/fix/login" opens as "fix/login" (`github::base_branch`).
    public static func withoutRemote(_ branch: String) -> String {
        branch.hasPrefix("origin/") ? String(branch.dropFirst(7)) : branch
    }

    static let adjectives = [
        "calm", "brave", "bright", "quick", "quiet", "eager", "gentle", "happy", "kind", "lively", "merry", "proud", "witty", "bold",
        "keen", "swift", "sunny", "clever", "fancy", "jolly", "lucky", "noble", "polite", "silly", "tidy", "vivid", "warm", "wise",
    ]
    static let nouns = [
        "otter", "lynx", "fox", "owl", "heron", "badger", "panda", "koala", "falcon", "beaver", "bison", "crane", "dolphin", "eagle",
        "ferret", "gecko", "hare", "ibis", "jaguar", "lemur", "marten", "newt", "orca", "puffin", "quail", "raven", "seal", "tapir",
    ]

    /// The first adjective-noun name from `seed` that nothing takes, as the desktop's petnames; "worktree", "worktree-2"… past them all.
    public static func freeName(seed: Int, taken: Set<String>) -> String {
        let count = adjectives.count * nouns.count
        for i in 0..<count {
            let n = (seed + i) % count
            let name = "\(adjectives[n / nouns.count])-\(nouns[n % nouns.count])"
            if !isTaken(name, taken) { return name }
        }
        return unique("worktree", taken)
    }

    /// `base`, or `base-2`, `base-3`… when taken.
    public static func unique(_ base: String, _ taken: Set<String>) -> String {
        if !isTaken(base, taken) { return base }
        var n = 2
        while isTaken("\(base)-\(n)", taken) { n += 1 }
        return "\(base)-\(n)"
    }

    /// The number in "123", "#123" or a pull request URL.
    public static func parsePR(_ s: String) -> Int? {
        let s = s.trimmingCharacters(in: .whitespacesAndNewlines)
        var n: Substring
        let scheme = ["https://", "http://"].first { s.hasPrefix($0) }
        if let scheme {
            let rest = s.dropFirst(scheme.count)
            let parts = rest.split(separator: "/", omittingEmptySubsequences: false)
            guard parts.count >= 5, parts[3] == "pull" else { return nil }
            n = parts[4].split(whereSeparator: { $0 == "?" || $0 == "#" }).first ?? parts[4]
        } else {
            n = s.hasPrefix("#") ? s.dropFirst() : Substring(s)
        }
        guard !n.isEmpty, n.utf8.allSatisfy({ $0 >= 0x30 && $0 <= 0x39 }), let v = Int(n), v > 0 else { return nil }
        return v
    }

    /// The branch menu's rows: the 20 newest local branches, or the first 50 local then remote branches holding `search`.
    /// Each carries whether it is local.
    public static func listed(local: [String], remote: [String], search: String) -> [(name: String, local: Bool)] {
        let q = search.trimmingCharacters(in: .whitespaces).lowercased()
        if q.isEmpty { return local.prefix(20).map { ($0, true) } }
        let all = local.map { ($0, true) } + remote.map { ($0, false) }
        return Array(all.filter { $0.0.lowercased().contains(q) }.prefix(50))
    }

    /// The base a new worktree starts from: the preferred one, else main, master, the current branch, else the newest.
    public static func defaultBase(_ branches: [String], preferred: String = "", current: String = "") -> String? {
        for name in [preferred, "main", "master", current] where !name.isEmpty && branches.contains(name) { return name }
        return branches.first
    }

    /// The folder pocketd will likely make: the name, the branch with / as -, or pr-<n>.
    public static func folder(name: String, branch: String? = nil, pr: Int? = nil) -> String {
        if let pr { return "pr-\(pr)" }
        if let branch { return branch.replacingOccurrences(of: "/", with: "-") }
        return name
    }
}
