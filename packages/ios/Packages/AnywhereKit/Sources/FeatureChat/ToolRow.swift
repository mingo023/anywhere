import ChatModel
import CodeView
import DesignSystem
import SwiftUI
import Wire

/// A tool call as one line: a chevron, its verb and what it acted on, and an edit's line counts. A failed call opens on
/// its error, an edit on its diff; successful output never shows.
@MainActor
struct ToolRow: View {
    let call: Call
    /// Running while its turn goes on.
    let live: Bool
    /// Still running after the work it was part of settled.
    let background: Bool
    let open: Bool
    let toggle: () -> Void

    private var failed: Bool { call.status == .failed }
    private var diff: (path: String, diff: FileDiff)? { failed ? nil : editDiff(call) }
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
            if open, let diff { EditDiffView(path: diff.path, diff: diff.diff) }
        }
    }

    private var line: some View {
        let arg = toolLine(call).arg
        let verb = call.status == .running ? toolLine(call).verb : pastVerb(call)
        return HStack(spacing: 6) {
            if failed { LucideIcon(.close, size: 13, weight: 2.5) } else { LucideIcon(open ? .chevronDown : .chevronRight, size: 13) }
            HStack(spacing: 6) {
                if !verb.isEmpty { Text(verb).foregroundStyle((failed ? Palette.failedText : Palette.text2).color) }
                switch arg {
                case .file(let path), .raw(let path): Text(path).font(ChatFont.toolArg).lineLimit(1).truncationMode(.middle)
                case .text(let text): Text(text).lineLimit(1).truncationMode(.tail)
                }
                if let counts = diff?.diff, counts.additions + counts.deletions > 0 {
                    Text("+\(counts.additions)").font(ChatFont.toolArg).foregroundStyle(Palette.successText.color)
                    Text("\u{2212}\(counts.deletions)").font(ChatFont.toolArg).foregroundStyle(Palette.failedText.color)
                }
            }
            .shimmer(live)
            if background { Text("background").foregroundStyle(Palette.text4.color) }
        }
        .font(ChatFont.small)
        .foregroundStyle((failed ? Palette.failedText : Palette.text3).color)
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
