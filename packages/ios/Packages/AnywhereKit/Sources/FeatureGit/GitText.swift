import DesignSystem
import DiffModel
import Foundation
import Stores
import Wire

/// The words the git screens show, as pure functions of their data.
enum GitText {
    /// "now", "5m", "3h", "2d", then the date: how long ago a commit was made.
    nonisolated static func age(_ at: Int, now: Int) -> String {
        let s = max(now - at, 0)
        switch s {
        case ..<60: return "now"
        case ..<3_600: return "\(s / 60)m"
        case ..<86_400: return "\(s / 3_600)h"
        case ..<(86_400 * 30): return "\(s / 86_400)d"
        default:
            let f = DateFormatter()
            f.locale = Locale(identifier: "en_US_POSIX")
            f.dateFormat = "d MMM yyyy"
            return f.string(from: Date(timeIntervalSince1970: TimeInterval(at)))
        }
    }

    /// "calm-otter → main": the branch and the base it's measured against.
    nonisolated static func branchLine(_ head: GitHead) -> String {
        let branch = head.detached ? "Detached HEAD" : head.branch ?? "No branch"
        return head.base.map { "\(branch) → \($0)" } ?? branch
    }

    /// "5 ahead · 12 behind main", against the base, not the upstream.
    nonisolated static func distance(_ head: GitHead) -> String? {
        guard let base = head.base else { return nil }
        return "\(head.ahead) ahead · \(head.behind) behind \(base)"
    }

    /// "4 commits since main", counting every one, though the status lists at most 20.
    nonisolated static func sinceBase(_ head: GitHead) -> String {
        "\(head.ahead) commit\(head.ahead == 1 ? "" : "s") since \(head.base ?? "base")"
    }

    /// What Push says: how many commits it sends, or that it publishes the branch.
    nonisolated static func push(_ head: GitHead) -> String? {
        guard !head.detached, head.branch != nil else { return nil }
        guard let unpushed = head.unpushed else { return "Publish Branch" }
        return unpushed > 0 ? "Push \(unpushed)" : nil
    }

    /// The confirm before a Push that replaces commits on the remote, the one the upstream is on; nil when Push needs none.
    nonisolated static func forcePush(_ head: GitHead) -> (title: String, message: String)? {
        guard let n = head.forceReplaces, let branch = head.branch else { return nil }
        let remote = head.upstream.flatMap { $0.split(separator: "/", maxSplits: 1).first }.map(String.init) ?? "the remote"
        return ("Force push \(branch)?", "Replaces \(n) commit\(n == 1 ? "" : "s") on \(remote).")
    }

    /// "Pull 3" when the upstream has commits this branch lacks; a plain Pull from a pocketd that doesn't count them.
    nonisolated static func pull(_ head: GitHead) -> String? {
        guard head.upstream != nil else { return nil }
        guard let unpulled = head.unpulled else { return "Pull" }
        return unpulled > 0 ? "Pull \(unpulled)" : nil
    }

    /// "Stage All 1,200", or "Staging 1,200…" while that runs.
    nonisolated static func stageAll(_ section: ChangeSection, count: Int, bulk: BulkStage?) -> String {
        let staged = section == .staged
        if let bulk, bulk.section == section { return "\(staged ? "Unstaging" : "Staging") \(bulk.count.formatted())…" }
        return "\(staged ? "Unstage All" : "Stage All") \(count.formatted())"
    }

    /// The desktop's discard facts: edits are lost, and untracked files among `paths` are deleted.
    nonisolated static func discardFacts(_ paths: [String], files: [GitFile]) -> String {
        let picked = Set(paths)
        let untracked = files.count { $0.status == .untracked && picked.contains($0.path) }
        let deletes = untracked > 0 ? "\nDeletes \(untracked) untracked file\(untracked == 1 ? "" : "s")" : ""
        return "Unstaged edits can't be restored" + deletes
    }

    /// "3 of 42" once a change is stepped to, else how many there are.
    nonisolated static func position(_ current: Int?, of count: Int) -> String {
        if let current, count > 0 { return "\(current + 1) of \(count)" }
        return count == 1 ? "1 change" : "\(count) changes"
    }

    /// The file's name and the folder it sits in, apart.
    nonisolated static func split(_ path: String) -> (name: String, dir: String) {
        guard let slash = path.lastIndex(of: "/") else { return (path, "") }
        return (String(path[path.index(after: slash)...]), String(path[..<slash]))
    }

    nonisolated static func status(_ s: FileStatus) -> String {
        switch s {
        case .modified: "Modified"
        case .added: "Added"
        case .deleted: "Deleted"
        case .renamed: "Renamed"
        case .conflict: "Conflict"
        case .untracked: "Untracked"
        default: s.rawValue
        }
    }

    nonisolated static func letter(_ s: FileStatus) -> String { s == .untracked ? "U" : s == .conflict ? "!" : s.rawValue }

    nonisolated static func tint(_ s: FileStatus) -> Token {
        switch s {
        case .added, .untracked: Palette.successText
        case .deleted: Palette.failedText
        case .renamed: Palette.link
        case .conflict: Palette.waitingText
        default: Palette.modified
        }
    }

    nonisolated static func counts(added: Int?, removed: Int?, binary: Bool) -> String? {
        if binary { return "binary" }
        guard let added, let removed else { return nil }
        return "+\(added) −\(removed)"
    }

    nonisolated static func short(_ sha: String) -> String { String(sha.prefix(7)) }

    /// What a diff compares, under its file's name.
    nonisolated static func comparison(_ at: DiffAt) -> String {
        switch at {
        case .working: "HEAD → working tree"
        case .staged: "HEAD → staged"
        case .commit(let rev): "\(short(rev))^ → \(short(rev))"
        case .base(let rev): "\(rev ?? "base") → HEAD"
        }
    }
}
