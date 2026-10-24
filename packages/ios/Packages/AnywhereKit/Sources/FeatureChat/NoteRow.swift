import ChatModel
import DesignSystem
import SwiftUI

/// Why the agent went on by itself, "Stop hook: 3 tests fail", or an error it went on after.
@MainActor
struct NoteRow: View {
    let text: String
    let error: Bool

    var body: some View {
        HStack(spacing: 6) {
            Image(systemName: error ? "exclamationmark.triangle" : "arrow.uturn.forward").font(.system(size: 11))
            Text(text).lineLimit(2)
        }
        .font(ChatFont.small)
        .foregroundStyle(Palette.text3.color)
    }
}

/// What finished in the background; a tap shows the call where it ran.
@MainActor
struct NoticeRow: View {
    let label: String
    let show: () -> Void

    var body: some View {
        Button {
            Haptics.tap()
            show()
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "bell").font(.system(size: 11))
                Self.text(label).lineLimit(2)
                Image(systemName: "arrow.up").font(.system(size: 9, weight: .semibold))
            }
            .font(ChatFont.small)
            .foregroundStyle(Palette.text3.color)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    /// The label with its backticked span in mono.
    static func text(_ label: String) -> Text {
        let (before, code, after) = codeSpan(label)
        guard let code else { return Text(verbatim: label) }
        return Text("\(Text(verbatim: before))\(Text(verbatim: code).font(ChatFont.mono))\(Text(verbatim: after))")
    }
}

/// Rules either side of "Context compacted".
@MainActor
struct CompactedRow: View {
    let auto: Bool

    var body: some View {
        HStack(spacing: Space.m) {
            Rectangle().fill(Palette.separator.color).frame(height: 1)
            Text(compactedLabel(auto: auto)).font(ChatFont.caption).foregroundStyle(Palette.text4.color).fixedSize()
            Rectangle().fill(Palette.separator.color).frame(height: 1)
        }
        .padding(.vertical, 14)
    }
}
