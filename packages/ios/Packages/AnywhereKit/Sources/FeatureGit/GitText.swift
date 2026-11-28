import DesignSystem
import DiffModel
import Domain
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

    /// The branch row has something to offer: a Push or Pull, the commits since its base, or a detached HEAD to say so.
    nonisolated static func showsHead(_ head: GitHead) -> Bool {
        push(head) != nil || pull(head) != nil || (head.base != nil && head.ahead > 0) || head.detached
    }

    /// "Stage All", or "Staging 1,200…" while that runs.
    nonisolated static func stageAll(_ section: ChangeSection, bulk: BulkStage?) -> String {
        let staged = section == .staged
        if let bulk, bulk.section == section { return "\(staged ? "Unstaging" : "Staging") \(bulk.count.formatted())…" }
        return staged ? "Unstage All" : "Stage All"
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

    /// "File 2 of 6": where the file sits among those the bar steps through.
    nonisolated static func fileOf(_ index: Int?, of count: Int) -> String {
        index.map { "File \($0 + 1) of \(count)" } ?? "\(count) file\(count == 1 ? "" : "s")"
    }

    /// A path cut to `max` characters by dropping the folders between its first and its file: "src/…/checkout.test.ts".
    nonisolated static func shortPath(_ path: String, max: Int = 32) -> String {
        guard path.count > max else { return path }
        let parts = path.split(separator: "/")
        guard parts.count > 2, let first = parts.first, let file = parts.last else { return path }
        let middle = "\(first)/…/\(file)"
        return middle.count <= max ? middle : "…/\(file)"
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

    /// The letter a changed file wears at the end of its row: M, A, D, R, U for untracked and C for a conflict.
    nonisolated static func letter(_ s: FileStatus) -> String {
        switch s {
        case .added: "A"
        case .untracked: "U"
        case .deleted: "D"
        case .renamed: "R"
        case .conflict: "C"
        default: "M"
        }
    }

    /// What heads a compare's files: the commits the branch is ahead of its base, or just how many files differ.
    nonisolated static func compareTitle(files: Int, ahead: Int?, against: String) -> String {
        guard let ahead else { return "\(files) file\(files == 1 ? "" : "s") differ from \(against)" }
        return "\(ahead) commit\(ahead == 1 ? "" : "s") ahead of \(against)"
    }

    /// A section's title over its card.
    nonisolated static func title(_ section: ChangeSection) -> String {
        section == .staged ? "Staged" : "Changes"
    }

    /// The filter box over the files shows once the list is long enough to need it, and while it holds a filter.
    nonisolated static func showsFilter(files: Int, filter: String) -> Bool {
        files >= 12 || !filter.trimmingCharacters(in: .whitespaces).isEmpty
    }

    /// The strip over a list kept from a Mac that has gone.
    nonisolated static func unreachable(mac: String, asOf: Int, timeZone: TimeZone = .current) -> String {
        "Can't reach \(mac) · as of \(TimeText.clock(ms: asOf, timeZone: timeZone))"
    }

    /// The commit button's words: its commit label and that it pushes after, or Amend.
    nonisolated static func commitButton(label: String, amend: Bool, busy: Bool) -> String {
        busy ? label : amend ? "Amend" : label + " & Push"
    }

    /// Whole calendar days from `at` to `now`, in `calendar`.
    nonisolated private static func days(from at: Int, to now: Int, calendar: Calendar) -> Int {
        let then = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(at)))
        let today = calendar.startOfDay(for: Date(timeIntervalSince1970: TimeInterval(now)))
        return calendar.dateComponents([.day], from: then, to: today).day ?? 0
    }

    nonisolated private static func format(_ at: Int, _ pattern: String, calendar: Calendar) -> String {
        let f = DateFormatter()
        f.locale = Locale(identifier: "en_US_POSIX")
        f.timeZone = calendar.timeZone
        f.dateFormat = pattern
        return f.string(from: Date(timeIntervalSince1970: TimeInterval(at)))
    }

    /// What follows "committed": "just now", "12m ago", "3h ago", "yesterday", "on Mon", then "on 12 Nov".
    nonisolated static func committed(at: Int, now: Int, calendar: Calendar = .current) -> String {
        let s = max(now - at, 0)
        if s < 60 { return "just now" }
        if s < 3_600 { return "\(s / 60)m ago" }
        switch days(from: at, to: now, calendar: calendar) {
        case 0: return "\(s / 3_600)h ago"
        case 1: return "yesterday"
        case 2..<7: return "on " + format(at, "EEE", calendar: calendar)
        default: return "on " + format(at, "d MMM", calendar: calendar)
        }
    }

    /// The day a history section is headed by: "Today", "Yesterday", then "Monday 23 November".
    nonisolated static func day(at: Int, now: Int, calendar: Calendar = .current) -> String {
        switch days(from: at, to: now, calendar: calendar) {
        case ...0: "Today"
        case 1: "Yesterday"
        default: format(at, "EEEE d MMMM", calendar: calendar)
        }
    }

    /// An author's two letters for the avatar: the first letters of two names, or the first two of one.
    nonisolated static func initials(_ author: String) -> String {
        let words = author.split(whereSeparator: \.isWhitespace)
        let letters = words.count > 1 ? words.prefix(2).compactMap(\.first) : Array((words.first ?? "").prefix(2))
        return String(letters).uppercased()
    }

    nonisolated static func tint(_ s: FileStatus) -> Token {
        switch s {
        case .added, .untracked: Palette.successText
        case .deleted: Palette.failedText
        case .renamed: Palette.link
        case .conflict: Palette.waitingText
        default: Palette.modified
        }
    }

    nonisolated static func short(_ sha: String) -> String { String(sha.prefix(7)) }
}
