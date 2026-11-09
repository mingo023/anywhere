import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A session in Up next or search results: what it asks, what failed or what it is, where it is, and when it last moved.
@MainActor
struct ActiveRow: View {
    let session: SessionModel
    /// Its project and branch.
    let place: String?
    /// Leads with its title, for a search that matched it, rather than what it asks or what failed.
    var titled = false
    /// Shows all of what it asks, as it's answered right under it.
    var whole = false
    @Environment(\.staleSince) private var stale

    var body: some View {
        let s = session.summary
        let status = session.status
        HStack(alignment: .top, spacing: Space.m) {
            StatusGlyph(status).padding(.top, 3)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(titled ? SessionText.title(s) : SessionText.headline(s, status, requests: session.requests))
                    .font(Typography.body.weight(.semibold))
                    .foregroundStyle(Palette.text.color)
                    .lineLimit(whole ? nil : 2)
                HStack(spacing: Space.xs) {
                    if let word = SessionText.outcome(status) {
                        Text(word).foregroundStyle((status == .failed ? Palette.failedText : Palette.successText).color)
                    }
                    if let word = SessionText.outcome(status), !word.isEmpty, place != nil { Text("·").foregroundStyle(Palette.text4.color) }
                    if let place {
                        Text(place).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                    } else if let detail = SessionText.look(s, status, requests: session.requests), status == .working {
                        Text(detail).foregroundStyle(Palette.text3.color)
                    }
                }
                .font(Typography.footnote)
                .lineLimit(1)
            }
            Spacer(minLength: Space.s)
            Elapsed(updatedAt: s.updatedAt, ticking: false).padding(.top, 2)
        }
        .padding(.vertical, Space.xxs)
        .opacity(stale == nil ? 1 : 0.55)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// A session in Working: where it runs, its current step, and how long it has been at it.
@MainActor
struct WorkingRow: View {
    let session: SessionModel
    let place: SessionPlace?
    @Environment(\.staleSince) private var stale

    var body: some View {
        let s = session.summary
        HStack(alignment: .top, spacing: Space.m) {
            StatusGlyph(.working).padding(.top, 3)
            VStack(alignment: .leading, spacing: Space.xxs) {
                HStack(spacing: Space.xs) {
                    Text(place?.project ?? SessionText.title(s)).font(Typography.subheadline.weight(.medium)).foregroundStyle(Palette.text.color)
                    if let tree = place?.tree {
                        Text("·").foregroundStyle(Palette.text4.color)
                        Text(tree).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                    }
                }
                .lineLimit(1)
                if let detail = SessionText.detail(s, .working) {
                    Text(detail).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1)
                        .shimmer(stale == nil && s.activity != nil)
                }
            }
            Spacer(minLength: Space.s)
            Elapsed(updatedAt: s.updatedAt, ticking: stale == nil).padding(.top, 2)
        }
        .padding(.vertical, Space.xxs)
        .opacity(stale == nil ? 1 : 0.55)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// A project: its initials, name, how many worktrees and Locals, when it last moved, and whether it's open.
@MainActor
struct ProjectRow: View {
    let section: HomeList.Section
    let line: String
    let ago: String
    let open: Bool
    let statuses: [Status]
    /// "1 failed, 2 done", for VoiceOver.
    let breakdown: String

    var body: some View {
        HStack(spacing: Space.m) {
            ProjectAvatar(name: section.name, statuses: statuses)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(section.name).font(Typography.headline).foregroundStyle(Palette.text.color).lineLimit(1)
                Text(line).font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
            }
            Spacer(minLength: Space.s)
            Text(ago).font(Typography.digits).foregroundStyle(Palette.text4.color)
            Image(systemName: open ? "chevron.down" : "chevron.right")
                .font(Typography.footnote.weight(.semibold))
                .foregroundStyle(Palette.text4.color)
                .frame(width: 16)
        }
        .padding(.vertical, Space.xxs)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(breakdown)
        .accessibilityHint(open ? "Hides its worktrees and Locals" : "Shows its worktrees and Locals")
    }
}

/// A project's initials on a tile, with the desktop's corner badges: a dot for what wants a look, a spinner for work.
@MainActor
struct ProjectAvatar: View {
    let name: String
    let statuses: [Status]
    var side = ProjectAvatar.size

    nonisolated static let size: CGFloat = 40

    var body: some View {
        RoundedRectangle(cornerRadius: side / 4, style: .continuous)
            .fill(Palette.fill3.color)
            .overlay(RoundedRectangle(cornerRadius: side / 4, style: .continuous).strokeBorder(Palette.separator.color))
            .overlay(Text(Workspaces.initials(name)).font(side < Self.size ? .system(size: 9, weight: .semibold, design: .monospaced) : Typography.captionStrong).foregroundStyle(Palette.text2.color))
            .frame(width: side, height: side)
            .overlay(alignment: .topTrailing) {
                if let alert = Workspaces.alert(statuses) {
                    Circle().fill(StatusGlyph.color(alert).color).frame(width: 9, height: 9)
                        .padding(2).background(Palette.background.color, in: Circle())
                        .offset(x: 4, y: -4)
                }
            }
            .overlay(alignment: .bottomTrailing) {
                if statuses.contains(.working) {
                    GradientSpinner(size: 10).frame(width: 12, height: 12).background(Palette.background.color, in: Circle())
                        .offset(x: 3, y: 3)
                }
            }
            .accessibilityHidden(true)
    }
}

/// A worktree or Local under its project: its status, name, branch, sessions, pull request, and when it moved.
@MainActor
struct WorkspaceRow: View {
    let tree: HomeList.Tree
    let ago: String
    /// Its branch's pull request, from the project's list.
    var pr: PrItem?

    static let insets = EdgeInsets(top: 0, leading: 16 + ProjectAvatar.size + Space.xs, bottom: 0, trailing: 20)

    var body: some View {
        HStack(alignment: .top, spacing: Space.s) {
            StatusGlyph(tree.rollUp?.status ?? .idle, size: 12).padding(.top, 3)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(tree.label).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                HStack(spacing: Space.xs) {
                    if let place = tree.place, place != tree.label {
                        Text(place).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).layoutPriority(-1)
                        Text("·").foregroundStyle(Palette.text4.color)
                    }
                    Text(Self.count(tree.sessions.count)).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                }
                .font(Typography.footnote)
                .lineLimit(1)
            }
            Spacer(minLength: Space.s)
            if let pr { PrBadge(item: pr) }
            Text(ago).font(Typography.digits).foregroundStyle(Palette.text4.color).padding(.top, 1)
        }
        .padding(.vertical, 10)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    static func count(_ n: Int) -> String {
        n == 0 ? "No sessions" : n == 1 ? "1 session" : "\(n) sessions"
    }
}

/// The last row under a project: a new worktree, or a session where the Mac doesn't let this phone make worktrees.
@MainActor
struct NewWorkspaceRow: View {
    let title: String

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "plus").font(Typography.footnote.weight(.semibold)).frame(width: 18)
            Text(title).font(Typography.subheadline)
            Spacer()
        }
        .foregroundStyle(Palette.text3.color)
        .padding(.vertical, 12)
        .contentShape(Rectangle())
    }
}

extension View {
    /// A row on the screen's own background rather than a card.
    func bareRow() -> some View {
        listRowBackground(Color.clear).listRowSeparatorTint(Palette.separator.color)
    }
}
