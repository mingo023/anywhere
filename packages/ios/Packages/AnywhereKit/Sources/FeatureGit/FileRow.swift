import DesignSystem
import SwiftUI
import Wire

/// One changed file: its icon column, name over folder, and at the end its status letter and line counts.
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
    /// A compare's files wear no status letter, only their counts.
    var letters = true

    var body: some View {
        let _ = BodyCount.hit("FileRow")
        let (name, dir) = GitText.split(path)
        HStack(spacing: 0) {
            LucideIcon(.fileText, size: 17).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
            VStack(alignment: .leading, spacing: 0) {
                Text(name)
                    .font(Typography.body)
                    .foregroundStyle((status == .deleted ? Palette.text3 : Palette.text).color)
                    .strikethrough(status == .deleted, color: Palette.text4.color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if let oldPath {
                    Text("from \(oldPath)").font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text2.color).lineLimit(1).truncationMode(.head)
                } else if !nameOnly, !dir.isEmpty {
                    Text(dir).font(Typography.caption).fontDesign(.monospaced).foregroundStyle(Palette.text2.color).lineLimit(1).truncationMode(.head)
                }
            }
            Spacer(minLength: Space.m)
            LineCounts(status: letters ? status : nil, added: added, removed: removed, binary: binary)
                .padding(.trailing, Space.l)
        }
        .padding(.leading, CGFloat(depth) * Space.l)
        .accessibilityElement(children: .combine)
    }
}

/// A file's status letter and its added and removed lines in their diff colours, or that it's binary.
@MainActor
struct LineCounts: View, Equatable {
    let status: FileStatus?
    let added: Int?
    let removed: Int?
    var binary = false

    var body: some View {
        HStack(spacing: 6) {
            if let status {
                Text(GitText.letter(status)).foregroundStyle(GitText.tint(status).color).accessibilityLabel(GitText.status(status))
            }
            if binary {
                Text("binary").foregroundStyle(Palette.text3.color)
            } else if let added, let removed {
                Text("+\(added)").foregroundStyle(Palette.successText.color)
                Text("−\(removed)").foregroundStyle(Palette.failedText.color)
            }
        }
        .font(Typography.codeSmall)
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
            LucideIcon(.chevronRight, size: 13).foregroundStyle(Palette.text4.color)
                .rotationEffect(.degrees(open ? 90 : 0))
                .frame(width: 16)
            LucideIcon(.folder, size: 17).foregroundStyle(Palette.text2.color)
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
