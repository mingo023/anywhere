import DesignSystem
import SwiftUI

/// A bar's centred title with a quieter mono line under it: "Files" over the worktree, a file's name over its folder.
@MainActor
struct BarHeading: View {
    let title: String
    let detail: String?

    var body: some View {
        VStack(spacing: 0) {
            Text(title).font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color).truncationMode(.middle)
            if let detail {
                Text(detail).font(.system(size: 12, design: .monospaced)).foregroundStyle(Palette.text2.color).truncationMode(.head)
            }
        }
        .lineLimit(1)
        .accessibilityElement(children: .combine)
    }
}

/// The round "…" of a bar's corner, for a menu.
@MainActor
struct BarMenuLabel: View {
    var body: some View {
        Label { Text("Options") } icon: { Image.lucide(.ellipsis, size: 20) }
    }
}

/// The design's two-way segmented control: a 36 pt capsule of the quiet fill, the picked half raised on it.
@MainActor
struct Segments: View {
    let options: [(value: Bool, title: String)]
    @Binding var selection: Bool

    var body: some View {
        HStack(spacing: 0) {
            ForEach(options, id: \.value) { option in
                let picked = option.value == selection
                Button { selection = option.value } label: {
                    Text(option.title)
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(Palette.text.color)
                        .lineLimit(1)
                        .frame(maxWidth: .infinity, maxHeight: .infinity)
                        .background { if picked { Capsule().fill(Palette.segment.color).shadow(color: .black.opacity(0.18), radius: 1.5, y: 1) } }
                        .contentShape(Capsule())
                }
                .buttonStyle(.plain)
                .accessibilityAddTraits(picked ? .isSelected : [])
            }
        }
        .padding(2)
        .frame(height: 36)
        .background(Palette.fill3.color, in: Capsule())
        .padding(.horizontal, Inset.margin)
    }
}

/// A file opened from a chat pins this under it: one press mentions the file there.
@MainActor
struct MentionBar: View {
    let mention: () -> Void

    var body: some View {
        Button(action: mention) {
            HStack(spacing: Space.s) {
                LucideIcon(.atSign, size: 20)
                Text("Mention in chat")
            }
            .font(.system(size: 17, weight: .semibold))
            .foregroundStyle(Palette.onText.color)
            .frame(maxWidth: .infinity)
            .frame(height: 52)
            .background(Palette.text.color, in: Capsule())
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .padding(.horizontal, 20)
        .padding(.top, Space.xl)
        .padding(.bottom, Space.s)
        .background(LinearGradient(colors: [Palette.background.color.opacity(0), Palette.background.color], startPoint: .top, endPoint: .init(x: 0.5, y: 0.45)))
    }
}

/// What a Files screen says when it has nothing to show: a title, why, and a way to try again.
@MainActor
struct FilesNote: View {
    let title: String
    let detail: String?
    let retry: (() -> Void)?

    var body: some View {
        VStack(spacing: 0) {
            Text(title).font(.system(size: 17, weight: .semibold)).foregroundStyle(Palette.text.color)
            if let detail {
                Text(detail).font(.system(size: 15)).foregroundStyle(Palette.text2.color).multilineTextAlignment(.center).padding(.top, 6)
            }
            if let retry {
                Button("Try again", action: retry).buttonStyle(PillButtonStyle(.secondary)).padding(.top, 18)
            }
        }
        .multilineTextAlignment(.center)
        .padding(.horizontal, 40)
        .padding(.bottom, 80)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .screenBackground()
    }
}
