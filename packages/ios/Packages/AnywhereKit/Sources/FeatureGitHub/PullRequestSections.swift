import DesignSystem
import Domain
import Wire

/// The PRs tab's groups, in the order they list.
enum PrSection: CaseIterable, Hashable, Sendable {
    case needsAction, readyToMerge, open, mergedRecently

    var title: String {
        switch self {
        case .needsAction: "Needs action"
        case .readyToMerge: "Ready to merge"
        case .open: "Open"
        case .mergedRecently: "Merged recently"
        }
    }

    /// Where a PR lists. A closed one lists nowhere: the design has no group for it.
    static func of(_ item: PrItem) -> PrSection? {
        switch item.state {
        case "MERGED": return .mergedRecently
        case "CLOSED": return nil
        default: break
        }
        if item.draft { return .open }
        switch item.gate {
        case .checksFailed, .conflicts, .changesRequested: return .needsAction
        case .ready: return .readyToMerge
        default: return .open
        }
    }
}

/// A PR on one Mac, with the project it is in: one row of the tab.
struct PrEntry: Hashable, Identifiable, Sendable {
    let mac: String
    let macName: String
    let projectPath: String
    let repo: String
    let item: PrItem

    var id: String { "\(mac)\n\(item.tree)" }
    var route: Route { .pullRequest(mac: mac, tree: item.tree) }
}

struct PrGroup: Identifiable, Hashable, Sendable {
    let section: PrSection
    let entries: [PrEntry]
    var id: PrSection { section }
}

extension PrSection {
    /// The row's state icon: red while it needs action, else as GitHub draws the PR's state.
    static func icon(_ item: PrItem) -> (icon: Lucide, tone: PrTone) {
        let glyph = PullRequestText.glyph(state: item.state, draft: item.draft)
        return of(item) == .needsAction ? (glyph.icon, .failed) : glyph
    }
}

/// The one small capsule a row offers.
enum PrRowAction: Hashable, Sendable {
    case fix, merge

    var title: String { self == .fix ? "Fix" : "Merge" }

    static func of(_ item: PrItem) -> Self? {
        switch PrSection.of(item) {
        case .needsAction?: .fix
        case .readyToMerge?: .merge
        default: nil
        }
    }
}

enum PrSections {
    /// The groups that have rows, each keeping the order the entries came in.
    static func group(_ entries: [PrEntry]) -> [PrGroup] {
        let bySection = Dictionary(grouping: entries) { PrSection.of($0.item) }
        return PrSection.allCases.compactMap { section in
            bySection[section].map { PrGroup(section: section, entries: $0) }
        }
    }

    static func needingAction(_ items: [PrItem]) -> Int {
        items.count { PrSection.of($0) == .needsAction }
    }
}

/// What a row's meta line says: the checks, in their colour, then plain words.
struct PrMeta: Hashable, Sendable {
    enum Checks: Hashable, Sendable {
        case failed(Int), running, passed

        var text: String {
            switch self {
            case .failed(let n): "\(n) failed"
            case .running: "Running"
            case .passed: "Passed"
            }
        }
    }

    var checks: Checks?
    var detail: String?

    /// "2 failed · Changes requested", "Passed", "Running · Draft". Pocketd's list carries no review state or diffstat,
    /// so what holds the merge stands in for the review.
    static func of(_ item: PrItem) -> Self {
        let c = item.checks
        let checks: Checks? = c.failed > 0 ? .failed(c.failed) : c.pending > 0 ? .running : c.passed > 0 ? .passed : nil
        return PrMeta(checks: checks, detail: detail(item))
    }

    private static func detail(_ item: PrItem) -> String? {
        if item.state == "MERGED" { return "Merged" }
        if item.draft { return "Draft" }
        return switch item.gate {
        case .conflicts: "Conflicts"
        case .changesRequested: "Changes requested"
        case .reviewRequired: "Review required"
        case .blocked: "Blocked"
        default: nil
        }
    }

    var isEmpty: Bool { checks == nil && detail == nil }
}

/// Why a Mac's PRs can't be listed or kept fresh.
struct MacPrProblems: Hashable, Sendable {
    let id: String
    let name: String
    let problems: [GhState]
}

struct GhNotice: Hashable, Identifiable, Sendable {
    let id: String
    let title: String
    let detail: String
}

enum PrProblems {
    /// One notice per Mac whose gh isn't installed or signed in: nothing of its PRs can load until that's fixed.
    static func notices(_ macs: [MacPrProblems]) -> [GhNotice] {
        macs.compactMap { mac in
            if mac.problems.contains(.missing) {
                GhNotice(id: mac.id, title: "GitHub CLI isn't installed on \(mac.name)", detail: "Run brew install gh, then gh auth login on the Mac to see its pull requests")
            } else if mac.problems.contains(.signedOut) {
                GhNotice(id: mac.id, title: "GitHub CLI isn't signed in on \(mac.name)", detail: "Run gh auth login on the Mac to see its pull requests")
            } else {
                nil
            }
        }
    }

    /// gh couldn't reach GitHub for some project, so its list is the last answer.
    static func unreachable(_ macs: [MacPrProblems]) -> Bool {
        macs.contains { $0.problems.contains(.failed) }
    }
}
