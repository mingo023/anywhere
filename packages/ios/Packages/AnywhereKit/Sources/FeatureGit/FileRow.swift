import DesignSystem
import SwiftUI
import Wire

/// A file's status as one tinted letter, as the desktop's Changes list marks it.
@MainActor
struct StatusLetter: View {
    let status: FileStatus

    var body: some View {
        Text(GitText.letter(status))
            .font(.system(.caption, design: .monospaced, weight: .bold))
            .foregroundStyle(GitText.tint(status).color)
            .frame(width: 16)
            .accessibilityLabel(GitText.status(status))
    }
}

/// One changed file: its letter, name and folder, and its line counts.
@MainActor
struct FileRow: View, Equatable {
    let path: String
    let oldPath: String?
    let status: FileStatus
    let counts: String?
    var depth = 0
    /// In the tree the folder is the row above, so only the name shows.
    var nameOnly = false

    var body: some View {
        let (name, dir) = GitText.split(path)
        HStack(spacing: Space.s) {
            StatusLetter(status: status)
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .font(Typography.body)
                    .foregroundStyle((status == .deleted ? Palette.text3 : Palette.text).color)
                    .strikethrough(status == .deleted, color: Palette.text4.color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let oldPath {
                    Text("from \(oldPath)").font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                } else if !nameOnly, !dir.isEmpty {
                    Text(dir).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                }
            }
            Spacer(minLength: Space.s)
            if let counts {
                Text(counts).font(Typography.digits).foregroundStyle(Palette.text3.color)
            }
        }
        .padding(.leading, CGFloat(depth) * Space.l)
        .accessibilityElement(children: .combine)
    }
}

/// A folder of the tree view; tapping it folds what it holds.
@MainActor
struct FolderRow: View, Equatable {
    let label: String
    let depth: Int
    let open: Bool

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "chevron.right")
                .font(.caption.weight(.semibold))
                .foregroundStyle(Palette.text4.color)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 16)
            Image(systemName: open ? "folder" : "folder.fill").foregroundStyle(Palette.text3.color)
            Text(label).font(Typography.subheadline).foregroundStyle(Palette.textBody.color).lineLimit(1).truncationMode(.head)
            Spacer()
        }
        .padding(.leading, CGFloat(depth) * Space.l)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(open ? "Open" : "Folded")
        .accessibilityAddTraits(.isButton)
    }
}
