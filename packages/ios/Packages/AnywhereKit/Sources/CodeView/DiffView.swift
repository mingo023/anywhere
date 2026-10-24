import DesignSystem
import DiffModel
import Highlight
import SwiftUI

/// A diff's rows, unified or side by side: hunk headers, add and delete tints, changed words, both line numbers,
/// no wrapping, scrolled in both directions. Only blocks of rows on screen are built, each drawn in one go, and each
/// colours itself off the main actor.
@MainActor
public struct DiffView: View {
    let document: DiffDocument
    let sideBySide: Bool
    @State private var spans: DiffSpans?
    @State private var scrollX: CGFloat = 0
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 20
    @ScaledMetric(relativeTo: .footnote) private var charWidth: CGFloat = 7.8

    public init(_ document: DiffDocument, sideBySide: Bool = false) {
        self.document = document
        self.sideBySide = sideBySide
    }

    public var body: some View {
        let _ = BodyCount.hit("DiffView")
        GeometryReader { geo in
            // The scroll view comes with its rows: one that starts empty centres its content, and keeps that offset
            // when the rows land, opening a long diff mid-way and off its left edge.
            if let spans {
                let count = sideBySide ? document.pairs.count : document.lines.count
                let column = max(geo.size.width / 2, CGFloat(min(document.widest, CodeStyle.maxChars) + 2) * charWidth + gutter)
                ScrollView([.vertical, .horizontal]) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(0..<DrawnRows.blocks(count), id: \.self) { b in
                            DiffBlock(
                                lines: document.lines, pairs: document.pairs, digits: document.gutterDigits, rows: DrawnRows.range(b, count: count),
                                sideBySide: sideBySide, column: column, minWidth: geo.size.width, scrollX: scrollX, rowHeight: rowHeight, spans: spans,
                                dark: scheme == .dark)
                        }
                    }
                    .padding(.bottom, Space.l)
                    .reportsScrollX(in: "diff")
                }
                .coordinateSpace(name: "diff")
                .onPreferenceChange(ScrollX.self) { x in MainActor.assumeIsolated { scrollX = x } }
                .defaultScrollAnchor(.topLeading)
            }
        }
        .background(Palette.surface.color)
        .task(id: DocumentKey(document)) {
            spans = DiffSpans(document.lines, language: document.language)
        }
    }

    private var gutter: CGFloat { CGFloat(document.gutterDigits) * charWidth }

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

/// A block of a diff's rows, unified or as side-by-side pairs, drawn in one `Canvas`.
@MainActor
struct DiffBlock: View {
    let lines: [UnifiedLine]
    var pairs: [SplitRow] = []
    let digits: Int
    let rows: Range<Int>
    var sideBySide = false
    var column: CGFloat = 0
    /// As wide as its widest row, at least this; without it the block takes the width it's offered and clips.
    let minWidth: CGFloat?
    /// How far the scroll view has moved right; the block draws only the screen's width from there.
    var scrollX: CGFloat = 0
    let rowHeight: CGFloat
    let spans: DiffSpans
    let dark: Bool
    @ScaledMetric(relativeTo: .footnote) private var size = DrawnText.footnote
    @ScaledMetric(relativeTo: .caption) private var hunkSize = DrawnText.caption

    var body: some View {
        let _ = BodyCount.hit("DiffBlock")
        let texts = DiffTexts(spans: spans, digits: digits, dark: dark, size: size, hunkSize: hunkSize)
        let cells = rows.map { sideBySide ? pair(pairs[$0], texts) : unified(lines[$0], texts) }
        let width = minWidth.map { min in
            sideBySide ? column * 2 + 1 : cells.reduce(min) { w, row in max(w, row.first.map { $0.inset + ($0.text?.width ?? 0) + Space.m } ?? 0) }
        }
        DrawnRows(rows: cells, rowHeight: rowHeight, width: width, window: minWidth.map { (scrollX, $0) })
            .onAppear { shown.forEach(spans.need) }
            .accessibilityElement(children: .contain)
            .accessibilityChildren {
                ForEach(shown, id: \.self) { Text(DiffRow.spoken(lines[$0])) }
            }
    }

    /// The lines this block shows, in order.
    private var shown: [Int] {
        guard sideBySide else { return Array(rows) }
        return rows.flatMap { r in [pairs[r].left, pairs[r].right].compactMap { $0 } }
    }

    private func unified(_ line: UnifiedLine, _ texts: DiffTexts) -> [Cell] {
        if line.kind == .hunk { return [Cell(fill: DiffRow.fill(.hunk, dark: dark), text: texts.hunk(line), inset: Space.xs + Space.m)] }
        return [Cell(fill: DiffRow.fill(line.kind, dark: dark), text: texts.row(line, .both), inset: Space.xs)]
    }

    private func pair(_ pair: SplitRow, _ texts: DiffTexts) -> [Cell] {
        if let i = pair.left, lines[i].kind == .hunk {
            return [Cell(width: column * 2, fill: DiffRow.fill(.hunk, dark: dark), text: texts.hunk(lines[i]), inset: Space.xs + Space.m)]
        }
        func half(_ index: Int?, _ side: CodeStyle.Gutter, x: CGFloat) -> Cell {
            guard let index else { return Cell(x: x, width: column, fill: DrawnText.cgColor(Palette.fill1.rgba(dark: dark))) }
            let line = lines[index]
            return Cell(x: x, width: column, fill: DiffRow.fill(line.kind, dark: dark), text: texts.row(line, side), inset: Space.xs)
        }
        return [
            half(pair.left, .old, x: 0),
            Cell(x: column, width: 1, fill: DrawnText.cgColor(Palette.separator.rgba(dark: dark))),
            half(pair.right, .new, x: column + 1),
        ]
    }
}

/// A diff's drawn rows, from the cache `DiffSpans` keeps, plain until their colours land.
@MainActor
private struct DiffTexts {
    let spans: DiffSpans
    let digits: Int
    let dark: Bool
    let size: CGFloat
    let hunkSize: CGFloat

    func row(_ line: UnifiedLine, _ gutter: CodeStyle.Gutter) -> DrawnText {
        spans.texts.text(DiffRowKey(row: line.id, gutter: gutter, dark: dark, size: size), coloured: spans.coloured(line.id)) {
            DrawnText(
                CodeStyle.diffRow(line, gutter: gutter, digits: digits, spans: spans.spans(line.id), dark: dark), size: size,
                color: Palette.text.rgba(dark: dark))
        }
    }

    func hunk(_ line: UnifiedLine) -> DrawnText {
        spans.texts.text(DiffRowKey(row: line.id, gutter: .both, dark: dark, size: hunkSize), coloured: true) {
            var runs = LineRuns()
            runs.append(line.text)
            return DrawnText(runs, size: hunkSize, color: Palette.text3.rgba(dark: dark))
        }
    }
}

/// How a diff row reads and what it's filled with.
enum DiffRow {
    nonisolated static func spoken(_ line: UnifiedLine) -> String {
        let cut = line.cut ? ", cut" : ""
        return switch line.kind {
        case .hunk: "Hunk \(line.text)"
        case .added: "Added line \(line.new ?? 0): \(line.text)\(cut)"
        case .deleted: "Deleted line \(line.old ?? 0): \(line.text)\(cut)"
        case .context: "Line \(line.new ?? 0): \(line.text)\(cut)"
        }
    }

    nonisolated static func fill(_ kind: UnifiedLine.Kind, dark: Bool) -> CGColor? {
        let token: Token? =
            switch kind {
            case .added: Palette.diffAddBackground
            case .deleted: Palette.diffDelBackground
            case .hunk: Palette.fill2
            case .context: nil
            }
        return token.map { DrawnText.cgColor($0.rgba(dark: dark)) }
    }
}
