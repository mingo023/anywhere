import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A session in Up next, Working or search results: what it is, where it is, what it asks or is doing, and when it
/// last moved.
@MainActor
struct ActiveRow: View {
    let session: SessionModel
    /// Its project, or project · worktree.
    let place: String?
    @Environment(\.staleSince) private var stale

    var body: some View {
        let s = session.summary
        let status = session.status
        let detail = SessionText.look(s, status, requests: session.requests)
        HStack(spacing: Space.m) {
            Group {
                if status == .needsYou {
                    ZStack {
                        Circle().fill(StatusGlyph.color(status).color.opacity(0.25)).frame(width: 18, height: 18)
                        Circle().fill(StatusGlyph.color(status).color).frame(width: 9, height: 9)
                    }
                } else {
                    StatusGlyph(status, size: 14)
                }
            }
            .frame(width: 18)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(SessionText.title(s)).font(Typography.body.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                HStack(spacing: Space.xs) {
                    if let place { Text(place).foregroundStyle(Palette.text3.color).layoutPriority(-1) }
                    if place != nil && detail != nil { Text("·").foregroundStyle(Palette.text4.color) }
                    if let detail {
                        Text(detail).foregroundStyle((status == .needsYou ? Palette.waitingText : status == .failed ? Palette.failedText : Palette.text3).color)
                    }
                }
                .font(Typography.footnote)
                .lineLimit(1)
            }
            Spacer(minLength: Space.s)
            Elapsed(updatedAt: s.updatedAt, ticking: false)
        }
        .padding(.vertical, Space.xxs)
        .opacity(stale == nil ? 1 : 0.55)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }
}

/// A project: its initials, name, how many worktrees and Locals and when it last moved, and what its sessions are doing.
@MainActor
struct ProjectRow: View {
    let section: HomeList.Section
    let line: String
    let open: Bool
    /// "1 failed, 2 done", for VoiceOver.
    let breakdown: String

    var body: some View {
        HStack(spacing: Space.m) {
            ProjectAvatar(name: section.name, status: section.rollUp?.status)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(section.name).font(Typography.headline).foregroundStyle(Palette.text.color).lineLimit(1)
                Text(line).font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1)
            }
            Spacer(minLength: Space.s)
            if let roll = section.rollUp, let label = Workspaces.label(roll.status, count: roll.count) {
                Pill(label, tone: SessionText.tone(roll.status)).accessibilityLabel(breakdown)
            }
            Image(systemName: open ? "chevron.down" : "chevron.right")
                .font(Typography.footnote.weight(.semibold))
                .foregroundStyle(Palette.text4.color)
                .frame(width: 16)
        }
        .padding(.vertical, Space.xxs)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityHint(open ? "Hides its worktrees and Locals" : "Shows its worktrees and Locals")
    }
}

/// A project's initials on a tile, badged with its most urgent session's status.
@MainActor
struct ProjectAvatar: View {
    let name: String
    let status: Status?

    nonisolated static let size: CGFloat = 40

    var body: some View {
        RoundedRectangle(cornerRadius: 10, style: .continuous)
            .fill(Palette.fill3.color)
            .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Palette.separator.color))
            .overlay(Text(Workspaces.initials(name)).font(Typography.captionStrong).foregroundStyle(Palette.text2.color))
            .frame(width: Self.size, height: Self.size)
            .overlay(alignment: .bottomTrailing) {
                if let status, status != .idle {
                    Circle().fill(StatusGlyph.color(status).color).frame(width: 11, height: 11)
                        .overlay(Circle().strokeBorder(Palette.surface.color, lineWidth: 2))
                        .offset(x: 3, y: 3)
                }
            }
            .accessibilityHidden(true)
    }
}

/// A worktree or Local under its project, joined to it by the tree line: its name, branch, sessions, and when it moved.
@MainActor
struct WorkspaceRow: View {
    let tree: HomeList.Tree
    let ago: String
    /// Its branch's pull request, from the project's list.
    var pr: PrItem?

    /// Flush top and bottom, so the tree line runs unbroken from row to row.
    static let insets = EdgeInsets(top: 0, leading: 16, bottom: 0, trailing: 20)

    var body: some View {
        let tint = tree.rollUp.map { StatusGlyph.color($0.status) } ?? Palette.text4
        HStack(alignment: .top, spacing: Space.s) {
            Image(systemName: TreeRow.symbol(tree.kind)).font(Typography.footnote).foregroundStyle(tint.color).frame(width: 18).padding(.top, 3)
            VStack(alignment: .leading, spacing: Space.xxs) {
                HStack(spacing: Space.xs) {
                    Text(tree.label).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                    if let pr { PrBadge(item: pr) }
                }
                HStack(spacing: Space.xs) {
                    if let place = tree.place, place != tree.label {
                        Text(place).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).layoutPriority(-1)
                        Text("·").foregroundStyle(Palette.text4.color)
                    }
                    Text(Self.count(tree.sessions.count)).foregroundStyle(Palette.text3.color)
                }
                .font(Typography.footnote)
                .lineLimit(1)
            }
            Spacer(minLength: Space.s)
            VStack(alignment: .trailing, spacing: 6) {
                Text(ago).font(Typography.digits).foregroundStyle(Palette.text4.color)
                Circle().fill(tint.color).frame(width: 7, height: 7)
            }
        }
        .padding(.vertical, 10)
        .padding(.leading, TreeLine.width)
        .background(alignment: .leading) { TreeLine(last: false).stroke(Palette.separatorStrong.color, lineWidth: 1).frame(width: TreeLine.width) }
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
    }

    static func count(_ n: Int) -> String {
        n == 0 ? "No sessions" : n == 1 ? "1 session" : "\(n) sessions"
    }
}

/// A quiet row under a project's trees: its pull requests, and last, a new worktree, or a session where the Mac doesn't
/// let this phone make worktrees.
@MainActor
struct NewWorkspaceRow: View {
    let title: String
    var systemImage = "plus"
    var last = true

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: systemImage).font(Typography.footnote.weight(.semibold)).frame(width: 18)
            Text(title).font(Typography.subheadline)
            Spacer()
        }
        .foregroundStyle(Palette.text3.color)
        .padding(.vertical, 12)
        .padding(.leading, TreeLine.width)
        .background(alignment: .leading) { TreeLine(last: last).stroke(Palette.separatorStrong.color, lineWidth: 1).frame(width: TreeLine.width) }
        .contentShape(Rectangle())
    }
}

/// The line from a project's avatar down its workspaces, curving into each; the last one ends it.
struct TreeLine: Shape {
    let last: Bool

    /// The gutter it draws in; the line runs down the avatar's middle.
    static let width: CGFloat = 40

    func path(in rect: CGRect) -> Path {
        let x = ProjectAvatar.size / 2
        let mid = rect.midY
        let radius: CGFloat = 8
        var path = Path()
        path.move(to: CGPoint(x: x, y: rect.minY))
        path.addLine(to: CGPoint(x: x, y: mid - radius))
        path.addQuadCurve(to: CGPoint(x: x + radius, y: mid), control: CGPoint(x: x, y: mid))
        path.addLine(to: CGPoint(x: x + 14, y: mid))
        if !last {
            path.move(to: CGPoint(x: x, y: mid - radius))
            path.addLine(to: CGPoint(x: x, y: rect.maxY))
        }
        return path
    }
}

extension View {
    /// A section's title as the home list sets them: sentence case, quieter than the rows.
    func sectionTitle() -> some View {
        font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text3.color).textCase(nil)
    }
}
