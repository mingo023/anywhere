import DesignSystem
import DiffModel
import Highlight
import SwiftUI
import Wire

/// An agent's edit inline in a chat: its hunks with both line numbers, add and delete tints, and the file's syntax
/// colours. Edits are short, so every row is built; long lines are clipped.
@MainActor
public struct EditDiffView: View {
    let lines: [UnifiedLine]
    let language: String
    @State private var spans: DiffSpans?
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 18
    @ScaledMetric(relativeTo: .footnote) private var charWidth: CGFloat = 7.8

    public init(path: String, diff: FileDiff) {
        lines = EditPreview.lines(diff)
        language = Highlighter.language(for: path)
    }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let spans {
                let gutter = CGFloat(DiffRows.digits(lines)) * charWidth
                ForEach(lines) { line in
                    // An overlay doesn't size the row, so a long line is clipped instead of widening every row.
                    DiffLineRow.fill(line.kind)
                        .frame(height: rowHeight)
                        .overlay(alignment: .leading) {
                            DiffLineRow(line: line, gutter: gutter, both: true, spans: spans, dark: scheme == .dark)
                        }
                        .clipped()
                }
            }
        }
        .padding(.vertical, Space.xs)
        .background(Palette.well.color, in: RoundedRectangle(cornerRadius: Radius.small))
        .clipShape(RoundedRectangle(cornerRadius: Radius.small))
        // Before the first frame, so the transcript doesn't jump when the rows land.
        .onAppear { if spans == nil { spans = DiffSpans(lines, language: language) } }
    }
}
