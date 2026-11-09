import DesignSystem
import FileIcons
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

/// One changed file: its letter, icon, name and folder, and its line counts.
@MainActor
struct FileRow: View, Equatable {
    let path: String
    let oldPath: String?
    let status: FileStatus
    let added: Int?
    let removed: Int?
    var binary = false
    var depth = 0
    /// In the tree the folder is the row above, so only the name shows.
    var nameOnly = false

    var body: some View {
        let _ = BodyCount.hit("FileRow")
        let (name, dir) = GitText.split(path)
        HStack(spacing: Space.s) {
            StatusLetter(status: status)
            MaterialIcon(path)
            VStack(alignment: .leading, spacing: 1) {
                Text(name)
                    .font(Typography.body)
                    .foregroundStyle((status == .deleted ? Palette.text3 : Palette.text).color)
                    .strikethrough(status == .deleted, color: Palette.text4.color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let oldPath {
                    Text("from \(oldPath)").font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                } else if !nameOnly, !dir.isEmpty {
                    Text(dir).font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                }
            }
            Spacer(minLength: Space.s)
            LineCounts(added: added, removed: removed, binary: binary)
        }
        .padding(.leading, CGFloat(depth) * Space.l)
        .accessibilityElement(children: .combine)
    }
}

/// A file's added and removed lines in their diff colours, or that it's binary.
@MainActor
struct LineCounts: View, Equatable {
    let added: Int?
    let removed: Int?
    var binary = false

    var body: some View {
        HStack(spacing: Space.xs) {
            if binary {
                Text("binary").foregroundStyle(Palette.text3.color)
            } else {
                if let added, added > 0 { Text("+\(added)").foregroundStyle(Palette.diffAddText.color) }
                if let removed, removed > 0 { Text("−\(removed)").foregroundStyle(Palette.diffDelText.color) }
            }
        }
        .font(Typography.digits)
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
            MaterialIcon(label, folder: true, open: open)
            Text(label).font(Typography.subheadline).fontDesign(.monospaced).foregroundStyle(Palette.textBody.color).lineLimit(1).truncationMode(.head)
            Spacer()
        }
        .padding(.leading, CGFloat(depth) * Space.l)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityValue(open ? "Open" : "Folded")
        .accessibilityAddTraits(.isButton)
    }
}
