import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A worktree's or Local's ways into its code, as one group of rows: Changes, its pull request and Files, each with what
/// it holds at its end. `tree` is the checkout they read: the worktree, or a Local's project checkout.
@MainActor
struct TreeLinks: View {
    let host: HostModel
    let mac: String
    let project: String
    let tree: String
    @Environment(\.staleSince) private var stale

    var body: some View {
        let head = host.git.changes(tree).head
        let item = host.github.lists[project]?.first { $0.tree == tree }
        let pr = Self.pullRequest(item, gh: host.github.status?.state, problem: host.github.pullRequest(tree).problem, readsGitHub: host.hello?.has(Caps.gh) == true)
        VStack(spacing: 0) {
            LinkRow(.fileDiff, "Changes", value: Route.changes(mac: mac, tree: tree)) { Self.changes(head?.totals) }
            if let pr {
                LinkRow(.pullRequest, pr, value: Route.pullRequest(mac: mac, tree: tree), divider: true) {
                    item.map(TreeText.pullRequest).map { Text($0.text).foregroundStyle(Self.color($0.tone).color) } ?? Text(stale == nil ? Self.unopened(head) ?? "None" : "None")
                }
            }
            LinkRow(.folder, "Files", value: Route.files(mac: mac, tree: tree, dir: ""), divider: true) { nil }
        }
        .insetGroup()
        .onAppear { host.git.show(tree) }
        .onDisappear { host.git.hide(tree) }
        .task {
            guard host.hello?.has(Caps.gh) == true else { return }
            // Only the tree's own answer says it has no GitHub remote.
            await host.github.refresh(tree)
            try? await host.github.refreshList(project)
        }
    }

    /// "+12 −3 · 4 files", "No changes"; nothing before the status loads.
    static func changes(_ totals: GitTotals?) -> Text? {
        guard let totals else { return nil }
        guard totals.files > 0 else { return Text("No changes") }
        return Text("+\(totals.added)").foregroundStyle(Palette.successText.color) + Text(" ")
            + Text("−\(totals.removed)").foregroundStyle(Palette.failedText.color) + Text(" · \(Self.files(totals.files))")
    }

    /// "1 file", "12 files".
    nonisolated static func files(_ count: Int) -> String {
        count == 1 ? "1 file" : "\(count) files"
    }

    /// What a pull request would hold, before there is one: "3 ahead of main".
    nonisolated static func unopened(_ head: GitHead?) -> String? {
        guard let head, let base = head.base, head.ahead > 0 else { return nil }
        return "\(head.ahead) ahead of \(base)"
    }

    /// The pull request row's title: "Create pull request" once gh can make one. Nil hides the row, for a repository with
    /// no GitHub remote or a Mac that can't read GitHub.
    nonisolated static func pullRequest(_ item: PrItem?, gh: GhState?, problem: GhState?, readsGitHub: Bool) -> String? {
        if !readsGitHub || problem == .noRemote { return nil }
        return item == nil && gh == .ready ? "Create pull request" : "Pull request"
    }

    private static func color(_ tone: Pill.Tone) -> Token {
        tone == .neutral ? Palette.text2 : Pill.colors(tone).text
    }
}

/// One way into a tree: an icon, a title, what it holds, and a chevron. The hairline above starts at the text.
@MainActor
private struct LinkRow: View {
    let icon: Lucide
    let title: String
    let value: Route
    let divider: Bool
    let detail: Text?

    init(_ icon: Lucide, _ title: String, value: Route, divider: Bool = false, detail: () -> Text?) {
        self.icon = icon
        self.title = title
        self.value = value
        self.divider = divider
        self.detail = detail()
    }

    var body: some View {
        PushRow(value: value) {
            HStack(spacing: 0) {
                LucideIcon(icon, size: 20).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
                Text(title).font(.system(size: 17)).foregroundStyle(Palette.text.color).lineLimit(1).layoutPriority(1)
                Spacer(minLength: Space.s)
                detail?.font(.system(size: 15).monospacedDigit()).foregroundStyle(Palette.text2.color).lineLimit(1).truncationMode(.head)
                LucideIcon(.chevronRight, size: 16).foregroundStyle(Palette.text4.color).padding(.leading, Space.xs).padding(.trailing, Space.m)
            }
            .frame(height: HitTarget.minimum)
            .contentShape(Rectangle())
            .rowDivider(divider)
        }
        .buttonStyle(.plain)
    }
}

/// What a tree's rows say, over plain data.
enum TreeText {
    /// A pull request as its row ends: "#42 · Checks failing", and how loudly to say it.
    static func pullRequest(_ item: PrItem) -> (text: String, tone: Pill.Tone) {
        let tone = PrBadge.tone(item)
        let state: String
        if item.state == "MERGED" {
            state = "Merged"
        } else if item.state == "CLOSED" {
            state = "Closed"
        } else if item.draft {
            state = "Draft"
        } else {
            switch item.gate {
            case .checksFailed: state = "Checks failing"
            case .conflicts: state = "Conflicts"
            case .changesRequested: state = "Changes requested"
            case .checksRunning: state = "Checks running"
            case .reviewRequired: state = "Review required"
            case .ready: state = "Ready to merge"
            default: state = "Open"
            }
        }
        return ("#\(item.number) · \(state)", tone)
    }

    /// "3 ahead", "2 behind", "up to date": where a branch stands once git has answered.
    static func standing(_ head: GitHead?) -> String? {
        guard let head else { return nil }
        if head.ahead > 0 { return "\(head.ahead) ahead" }
        if head.behind > 0 { return "\(head.behind) behind" }
        return "up to date"
    }
}
