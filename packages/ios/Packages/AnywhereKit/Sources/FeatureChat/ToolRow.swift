import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// A tool call as one line: its verb and what it acted on. A failed call opens on its error, an edit on its diff;
/// successful output never shows.
@MainActor
struct ToolRow: View {
    let call: Call
    /// Still running after the work it was part of settled.
    let background: Bool
    let open: Bool
    let toggle: () -> Void

    private var failed: Bool { call.status == .failed }
    private var diff: (path: String, diff: [DiffLine])? { failed ? nil : editDiff(call) }
    private var opens: Bool { failed ? !(call.output ?? "").isEmpty : diff != nil }

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button {
                Haptics.tap()
                toggle()
            } label: {
                line.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!opens)
            if open, failed, let output = call.output { ErrorOutput(lines: DetailLines.output(output)) }
            if open, let diff { InlineDiff(rows: diffRows(diff.diff)) }
        }
    }

    private var line: some View {
        let (verb, arg) = toolLine(call)
        let tint = failed ? Palette.failedText : Palette.text3
        return HStack(spacing: 6) {
            if failed { Image(systemName: "xmark").font(.system(size: 10, weight: .bold)) }
            HStack(spacing: 6) {
                if !verb.isEmpty { Text(verb) }
                switch arg {
                case .file(let path): FileChip(path, failed: failed)
                case .text(let text): Text(text).lineLimit(1).truncationMode(.tail)
                case .raw(let text): Text(text).font(ChatFont.mono).lineLimit(1).truncationMode(.tail)
                }
            }
            .shimmer(call.status == .running && !background)
            if background { Text("background").foregroundStyle(Palette.text4.color) }
            if opens { Image(systemName: open ? "chevron.down" : "chevron.right").font(.system(size: 9, weight: .semibold)) }
        }
        .font(ChatFont.small)
        .foregroundStyle(tint.color)
        .frame(minHeight: 24)
    }
}

/// A failed tool's output: mono, the first 24 lines, scrolling past 240 pt.
@MainActor
struct ErrorOutput: View {
    let lines: DetailLines

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                Text(lines.lines.joined(separator: "\n")).textSelection(.enabled)
                if let more = lines.moreLabel { Text(more).foregroundStyle(Palette.text4.color) }
            }
            .font(ChatFont.mono)
            .lineSpacing(8)
            .foregroundStyle(Palette.failedText.color)
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(Space.s)
        }
        .frame(maxHeight: 240)
        .fixedSize(horizontal: false, vertical: true)
        .background(Palette.failedBackground.color, in: RoundedRectangle(cornerRadius: Radius.small))
    }
}

/// An edit's lines in git-style hunks, 12 pt mono, long lines clipped.
@MainActor
struct InlineDiff: View {
    let rows: [ChatModel.DiffRow]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(rows.enumerated()), id: \.offset) { _, row in
                HStack(spacing: 6) {
                    Text(row.old.map(String.init) ?? "").frame(width: 30, alignment: .trailing)
                    Text(row.new.map(String.init) ?? "").frame(width: 30, alignment: .trailing)
                    Text(sign(row.kind)).frame(width: 8)
                    Text(row.text).lineLimit(1).truncationMode(.tail)
                }
                .foregroundStyle(color(row.kind).color)
                .frame(maxWidth: .infinity, minHeight: 18, alignment: .leading)
                .background(background(row.kind))
            }
        }
        .font(ChatFont.mono)
        .padding(.vertical, 4)
        .background(Palette.well.color, in: RoundedRectangle(cornerRadius: Radius.small))
        .clipShape(RoundedRectangle(cornerRadius: Radius.small))
    }

    private func sign(_ kind: ChatModel.DiffRow.Kind) -> String {
        switch kind {
        case .add: "+"
        case .del: "-"
        default: ""
        }
    }

    private func color(_ kind: ChatModel.DiffRow.Kind) -> Token {
        switch kind {
        case .add: Palette.diffAddText
        case .del: Palette.diffDelText
        case .hunk: Palette.text4
        case .context: Palette.textBody
        }
    }

    private func background(_ kind: ChatModel.DiffRow.Kind) -> Color {
        switch kind {
        case .add: Palette.diffAddBackground.color
        case .del: Palette.diffDelBackground.color
        default: .clear
        }
    }
}
