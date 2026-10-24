import DesignSystem
import DiffModel
import Highlight
import SwiftUI

/// A diff's rows, unified or side by side: hunk headers, add and delete tints, changed words, both line numbers,
/// no wrapping, scrolled in both directions. Only rows on screen are built, and each colours itself off the main actor.
@MainActor
public struct DiffView: View {
    let document: DiffDocument
    let sideBySide: Bool
    @State private var spans: DiffSpans?
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 20
    @ScaledMetric(relativeTo: .footnote) private var charWidth: CGFloat = 7.8

    public init(_ document: DiffDocument, sideBySide: Bool = false) {
        self.document = document
        self.sideBySide = sideBySide
    }

    public var body: some View {
        GeometryReader { geo in
            ScrollView([.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    if let spans {
                        if sideBySide {
                            let column = max(geo.size.width / 2, CGFloat(min(document.widest, CodeStyle.maxChars) + 2) * charWidth + gutter)
                            ForEach(document.pairs) { pair in
                                pairRow(pair, column: column, spans: spans)
                            }
                        } else {
                            ForEach(document.lines) { line in
                                DiffLineRow(line: line, gutter: gutter, both: true, spans: spans, dark: scheme == .dark)
                                    .frame(height: rowHeight)
                                    .frame(minWidth: geo.size.width, alignment: .leading)
                                    .background(DiffLineRow.fill(line.kind))
                            }
                        }
                    }
                }
                .padding(.bottom, Space.l)
            }
        }
        .background(Palette.surface.color)
        .task(id: DocumentKey(document)) {
            spans = DiffSpans(document.lines, language: document.language)
        }
    }

    private var gutter: CGFloat { CGFloat(document.gutterDigits) * charWidth }

    @ViewBuilder
    private func pairRow(_ pair: SplitRow, column: CGFloat, spans: DiffSpans) -> some View {
        let lines = document.lines
        if let i = pair.left, lines[i].kind == .hunk {
            DiffLineRow(line: lines[i], gutter: gutter, both: false, spans: spans, dark: scheme == .dark)
                .frame(width: column * 2, height: rowHeight, alignment: .leading)
                .background(DiffLineRow.fill(.hunk))
        } else {
            HStack(spacing: 0) {
                half(pair.left, side: .deleted, column: column, spans: spans)
                Rectangle().fill(Palette.separator.color).frame(width: 1)
                half(pair.right, side: .added, column: column, spans: spans)
            }
            .frame(height: rowHeight)
        }
    }

    @ViewBuilder
    private func half(_ index: Int?, side: UnifiedLine.Kind, column: CGFloat, spans: DiffSpans) -> some View {
        if let index {
            let line = document.lines[index]
            DiffLineRow(line: line, gutter: gutter, both: false, oldSide: side == .deleted, spans: spans, dark: scheme == .dark)
                .frame(width: column, alignment: .leading)
                .clipped()
                .background(DiffLineRow.fill(line.kind))
        } else {
            Palette.fill1.color.frame(width: column)
        }
    }

    /// A new document gets new colours; the same one keeps them across redraws.
    private struct DocumentKey: Equatable {
        let path: String
        let count: Int
        let added: Int
        let removed: Int
        let first: String?
        let last: String?
        init(_ d: DiffDocument) {
            path = d.path
            count = d.lines.count
            added = d.added
            removed = d.removed
            first = d.lines.first?.text
            last = d.lines.last?.text
        }
    }
}

/// One diff row: its numbers, its sign and its coloured text.
@MainActor
struct DiffLineRow: View {
    let line: UnifiedLine
    let gutter: CGFloat
    /// Unified rows show both numbers; a side-by-side half shows its own side's.
    let both: Bool
    var oldSide = false
    let spans: DiffSpans
    let dark: Bool

    var body: some View {
        HStack(spacing: Space.s) {
            if line.kind == .hunk {
                Text(line.text)
                    .font(Typography.caption.monospaced())
                    .foregroundStyle(Palette.text3.color)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                    .padding(.leading, Space.m)
            } else {
                if both {
                    LineNumber(number: line.old, width: gutter)
                    LineNumber(number: line.new, width: gutter)
                } else {
                    LineNumber(number: oldSide ? line.old : line.new, width: gutter)
                }
                Text(Self.sign(line.kind))
                    .font(CodeStyle.font)
                    .foregroundStyle(Self.signColor(line.kind).color)
                    .fixedSize()
                    .accessibilityHidden(true)
                Text(text)
                    .font(CodeStyle.font)
                    .foregroundStyle(Palette.text.color)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
                if line.cut {
                    Image(systemName: "ellipsis").font(Typography.caption).foregroundStyle(Palette.text4.color).accessibilityLabel("Line cut")
                }
            }
        }
        .padding(.trailing, Space.m)
        .padding(.leading, Space.xs)
        .onAppear { spans.need(line.id) }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(Self.spoken(line))
    }

    private var text: AttributedString {
        let word: Color? =
            switch line.kind {
            case .added: Palette.diffAddWord.color
            case .deleted: Palette.diffDelWord.color
            default: nil
            }
        return CodeStyle.attributed(line.text, spans: spans.spans(line.id), words: line.words, word: word, dark: dark)
    }

    nonisolated static func sign(_ kind: UnifiedLine.Kind) -> String {
        switch kind {
        case .added: "+"
        case .deleted: "−"
        default: " "
        }
    }

    nonisolated static func spoken(_ line: UnifiedLine) -> String {
        switch line.kind {
        case .hunk: "Hunk \(line.text)"
        case .added: "Added line \(line.new ?? 0): \(line.text)"
        case .deleted: "Deleted line \(line.old ?? 0): \(line.text)"
        case .context: "Line \(line.new ?? 0): \(line.text)"
        }
    }

    static func signColor(_ kind: UnifiedLine.Kind) -> Token {
        switch kind {
        case .added: Palette.diffAddText
        case .deleted: Palette.diffDelText
        default: Palette.text4
        }
    }

    static func fill(_ kind: UnifiedLine.Kind) -> Color {
        switch kind {
        case .added: Palette.diffAddBackground.color
        case .deleted: Palette.diffDelBackground.color
        case .hunk: Palette.fill2.color
        case .context: .clear
        }
    }
}
