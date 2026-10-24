import DesignSystem
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

    /// "↑2 ↓1" against the base, or nothing when level.
    nonisolated static func aheadBehind(_ head: GitHead) -> String? {
        let parts = [head.ahead > 0 ? "↑\(head.ahead)" : nil, head.behind > 0 ? "↓\(head.behind)" : nil].compactMap { $0 }
        return parts.isEmpty ? nil : parts.joined(separator: " ")
    }

    /// What Push says: how many commits it sends, or that it publishes the branch.
    nonisolated static func push(_ head: GitHead) -> String? {
        guard !head.detached, head.branch != nil else { return nil }
        guard let unpushed = head.unpushed else { return "Publish Branch" }
        return unpushed > 0 ? "Push \(unpushed)" : nil
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
