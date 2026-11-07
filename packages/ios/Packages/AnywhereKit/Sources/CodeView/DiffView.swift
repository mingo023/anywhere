import DesignSystem
import DiffModel
import Highlight
import SwiftUI

/// A diff's rows, unified or side by side: hunk headers, add and delete tints, changed words, line numbers. Unwrapped
/// it scrolls both ways; wrapped (unified only) it fits the width, with one number per row on a narrow screen. Only
/// blocks of rows on screen are built, each drawn in one go, and each colours itself off the main actor.
@MainActor
public struct DiffView: View {
    let document: DiffDocument
    let sideBySide: Bool
    let wrap: Bool
    let scrollTo: Int?
    let onHunk: ((Int) -> Void)?
    let onAsk: ((CodeQuote) -> Void)?
    @State private var shown: Shown?
    @State private var pick: LinePick?
    @State private var handle: (edge: LinePick.Edge, from: Int)?
    @State private var mark: Mark?
    @State private var shownTarget: Int?
    @State private var scrollX: CGFloat = 0
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 20
    @ScaledMetric(relativeTo: .footnote) private var charWidth: CGFloat = 7.8
    @ScaledMetric(relativeTo: .footnote) private var size = DrawnText.footnote

    /// The document on screen with its colours, and its rows when wrapped, replaced together so a changed diff keeps
    /// its scroll view and place while its rows are cut.
    private struct Shown {
        let document: DiffDocument
        let spans: DiffSpans
        var wrapped: (columns: Int, rows: CodeRows)?
    }

    private struct LayoutKey: Equatable {
        let document: DocumentKey?
        let columns: Int?
    }

    /// `scrollTo` is an index into the document's lines, brought near the top. `onHunk` gets a tapped hunk header's
    /// index. `onAsk` adds Ask in Chat to picked lines.
    public init(
        _ document: DiffDocument, sideBySide: Bool = false, wrap: Bool = false, scrollTo: Int? = nil, onHunk: ((Int) -> Void)? = nil,
        onAsk: ((CodeQuote) -> Void)? = nil
    ) {
        self.document = document
        self.sideBySide = sideBySide
        self.wrap = wrap
        self.scrollTo = scrollTo
        self.onHunk = onHunk
        self.onAsk = onAsk
    }

    private var wraps: Bool { wrap && !sideBySide }

    public var body: some View {
        let _ = BodyCount.hit("DiffView")
        GeometryReader { geo in
            let gutter: CodeStyle.Gutter = wraps && geo.size.width < 700 ? .one : .both
            let columns = wraps ? columns(gutter, width: geo.size.width) : nil
            // The scroll view comes with its rows: one that starts empty centres its content, and keeps that offset
            // when the rows land, opening a long diff mid-way and off its left edge.
            Group {
                if let shown, let rows = rows(shown) {
                    content(shown, rows: rows, gutter: gutter, width: geo.size.width)
                } else {
                    Color.clear
                }
            }
            .task(id: LayoutKey(document: shown.map { DocumentKey($0.document) }, columns: columns)) { await layOut(columns) }
        }
        .background(Palette.surface.color)
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let pick, let lines = shown?.document.lines, let quote = CodeQuote.diff(lines, pick: pick.lines) {
                let copy = lines[pick.lines].filter { $0.kind != .hunk }.map(\.text).joined(separator: "\n")
                PickBar(quote: quote, copyText: copy, ask: onAsk) { self.pick = nil }
            }
        }
        .task(id: DocumentKey(document)) { await load() }
        .task(id: TargetKey(target: scrollTo, ready: shown.flatMap(rows) != nil)) { showTarget() }
    }

    /// The rows drawn: pairs side by side, lines unwrapped, or the lines cut to the width; nil while those are cut.
    private func rows(_ shown: Shown) -> CodeRows? {
        if sideBySide { return CodeRows(lineCount: shown.document.pairs.count) }
        guard wrap else { return CodeRows(lineCount: shown.document.lines.count) }
        return shown.wrapped?.rows
    }

    private func content(_ shown: Shown, rows: CodeRows, gutter: CodeStyle.Gutter, width: CGFloat) -> some View {
        let document = shown.document
        let column = max(width / 2, CGFloat(min(document.widest, CodeStyle.maxChars) + 2) * charWidth + CGFloat(document.gutterDigits) * charWidth)
        let picked = pick?.lines
        return ScrollViewReader { proxy in
            ScrollView(wraps ? .vertical : [.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(0..<DrawnRows.blocks(rows.count), id: \.self) { b in
                        DiffBlock(
                            lines: document.lines, pairs: document.pairs, digits: document.gutterDigits, rows: DrawnRows.range(b, count: rows.count),
                            layout: sideBySide ? nil : rows, gutter: gutter, wrap: wraps, sideBySide: sideBySide, column: column, minWidth: width,
                            scrollX: scrollX, rowHeight: rowHeight, spans: shown.spans, dark: scheme == .dark, picked: picked,
                            handles: sideBySide ? nil : picked.map { handleRows($0, rows: rows) }, mark: mark,
                            actions: actions(document.lines, rows: rows))
                    }
                }
                .padding(.bottom, Space.l)
                .reportsScrollX(in: "diff")
            }
            .coordinateSpace(name: "diff")
            .onPreferenceChange(ScrollX.self) { x in MainActor.assumeIsolated { scrollX = x } }
            .defaultScrollAnchor(.topLeading)
            .task(id: mark) {
                guard let mark else { return }
                proxy.scrollTo(mark.row / DrawnRows.block, anchor: .topLeading)
                // The row's block is built only once scrolled to; its marker can't be reached before.
                try? await Task.sleep(for: .milliseconds(40))
                proxy.scrollTo(Mark.id, anchor: UnitPoint(x: 0, y: 0.2))
            }
        }
    }

    private func actions(_ lines: [UnifiedLine], rows: CodeRows) -> DiffActions {
        DiffActions(
            tapped: { i in
                if pick != nil {
                    pick = nil
                } else if let i, lines[i].kind == .hunk {
                    onHunk?(i)
                }
            },
            pressed: { i in
                guard lines[i].kind != .hunk else { return }
                pick = LinePick(i)
                Haptics.tap()
            },
            dragged: { i in _ = pick?.drag(to: i) { LinePick.sameHunk(lines, $0, $1) } },
            handleMoved: { edge, by in moveHandle(edge, by: by, lines: lines, rows: rows) },
            handleEnded: { handle = nil })
    }

    private func handleRows(_ picked: ClosedRange<Int>, rows: CodeRows) -> (top: Int, bottom: Int) {
        let top = handle.flatMap { $0.edge == .top ? $0.from : nil } ?? rows.first(of: picked.lowerBound)
        let bottom = handle.flatMap { $0.edge == .bottom ? $0.from : nil } ?? rows.rows(of: picked.upperBound).upperBound - 1
        return (top, bottom)
    }

    private func moveHandle(_ edge: LinePick.Edge, by: Int, lines: [UnifiedLine], rows: CodeRows) {
        guard let picked = pick?.lines else { return }
        let from = handle?.from ?? (edge == .top ? rows.first(of: picked.lowerBound) : rows.rows(of: picked.upperBound).upperBound - 1)
        if handle == nil { handle = (edge, from) }
        let row = (from + by).clamped(to: 0...max(rows.count - 1, 0))
        _ = pick?.move(edge, to: rows[row].line) { LinePick.sameHunk(lines, $0, $1) }
    }

    private func columns(_ gutter: CodeStyle.Gutter, width: CGFloat) -> Int {
        max(Int((width - Space.xs - Space.m) / DrawnText.advance(size)) - CodeStyle.diffCodeStart(gutter, digits: document.gutterDigits), 8)
    }

    private nonisolated static func cut(_ document: DiffDocument, columns: Int) async -> CodeRows {
        await Task.detached(priority: .userInitiated) {
            CodeRows(document.lines.map { $0.kind == .hunk ? "" : $0.text }, columns: columns, limit: nil)
        }.value
    }

    private func load() async {
        let document = document
        var wrapped: (columns: Int, rows: CodeRows)?
        if wraps, let columns = shown?.wrapped?.columns { wrapped = (columns, await Self.cut(document, columns: columns)) }
        guard !Task.isCancelled else { return }
        pick = nil
        shown = Shown(document: document, spans: DiffSpans(document.lines, language: document.language), wrapped: wrapped)
    }

    private func layOut(_ columns: Int?) async {
        guard let columns, let document = shown?.document, shown?.wrapped?.columns != columns else { return }
        let rows = await Self.cut(document, columns: columns)
        if !Task.isCancelled, shown.map({ DocumentKey($0.document) }) == DocumentKey(document) { shown?.wrapped = (columns, rows) }
    }

    private func showTarget() {
        guard let scrollTo, scrollTo != shownTarget, let shown, let rows = rows(shown), shown.document.lines.indices.contains(scrollTo) else { return }
        shownTarget = scrollTo
        let row = sideBySide ? shown.document.pairs.firstIndex { $0.left == scrollTo || $0.right == scrollTo } ?? 0 : rows.first(of: scrollTo)
        mark = Mark(row: row, x: 0, serial: (mark?.serial ?? 0) + 1)
    }

    private struct TargetKey: Equatable {
        let target: Int?
        let ready: Bool
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

/// What a diff's rows do when touched, each given the line (an index into the document's lines) under the finger.
struct DiffActions {
    let tapped: (Int?) -> Void
    let pressed: (Int) -> Void
    let dragged: (Int) -> Void
    let handleMoved: (LinePick.Edge, Int) -> Void
    let handleEnded: () -> Void
}

/// A block of a diff's rows, unified or as side-by-side pairs, drawn in one `Canvas`.
@MainActor
struct DiffBlock: View {
    let lines: [UnifiedLine]
    var pairs: [SplitRow] = []
    let digits: Int
    /// Rows of `layout`, or of `pairs` side by side, or else of `lines`.
    let rows: Range<Int>
    /// A unified diff's rows when they aren't one per line.
    var layout: CodeRows?
    var gutter: CodeStyle.Gutter = .both
    var wrap = false
    var sideBySide = false
    var column: CGFloat = 0
    /// As wide as its widest row, at least this; without it the block takes the width it's offered and clips.
    let minWidth: CGFloat?
    /// How far the scroll view has moved right; the block draws only the screen's width from there.
    var scrollX: CGFloat = 0
    let rowHeight: CGFloat
    let spans: DiffSpans
    let dark: Bool
    var picked: ClosedRange<Int>?
    var handles: (top: Int, bottom: Int)?
    var mark: Mark?
    var actions: DiffActions?
    @ScaledMetric(relativeTo: .footnote) private var size = DrawnText.footnote
    @ScaledMetric(relativeTo: .caption) private var hunkSize = DrawnText.caption

    var body: some View {
        let _ = BodyCount.hit("DiffBlock")
        let texts = DiffTexts(spans: spans, digits: digits, dark: dark, size: size, hunkSize: hunkSize)
        let cells = rows.map { sideBySide ? pair(pairs[$0], texts) : unified($0, texts) }
        let width = minWidth.map { min in
            sideBySide ? column * 2 + 1 : wrap ? min : cells.reduce(min) { w, row in max(w, row.last.map { $0.inset + ($0.text?.width ?? 0) + Space.m } ?? 0) }
        }
        let drawn = DrawnRows(rows: cells, rowHeight: rowHeight, width: width, window: wrap ? nil : minWidth.map { (scrollX, $0) })
        Group {
            if let actions {
                drawn
                    .overlay(alignment: .topLeading) { overlay(actions) }
                    .contentShape(Rectangle())
                    .onTapGesture(coordinateSpace: .local) { actions.tapped(line(at: $0)) }
                    .onLinePress(began: { if let i = line(at: $0) { actions.pressed(i) } }, moved: { if let i = line(at: $0) { actions.dragged(i) } }, ended: {})
            } else {
                drawn
            }
        }
        .onAppear { shown.forEach(spans.need) }
        .accessibilityElement(children: .contain)
        .accessibilityChildren {
            ForEach(shown, id: \.self) { Text(DiffRow.spoken(lines[$0])) }
        }
    }

    private func index(_ row: Int) -> (line: Int, part: Range<Int>?) {
        layout.map { ($0[row].line, $0[row].range) } ?? (row, nil)
    }

    /// The line under `point`: a side-by-side half's own, or the unified row's.
    private func line(at point: CGPoint) -> Int? {
        let count = sideBySide ? pairs.count : layout?.count ?? lines.count
        let row = (rows.lowerBound + Int((point.y / rowHeight).rounded(.down))).clamped(to: 0...max(count - 1, 0))
        guard sideBySide else { return index(row).line }
        let pair = pairs[row]
        if let left = pair.left, lines[left].kind == .hunk { return left }
        return point.x < column ? pair.left : pair.right
    }

    private func y(_ row: Int) -> CGFloat { CGFloat(row - rows.lowerBound) * rowHeight }

    @ViewBuilder
    private func overlay(_ actions: DiffActions) -> some View {
        if let mark, rows.contains(mark.row) {
            Color.clear.frame(width: 1, height: rowHeight).offset(y: y(mark.row)).id(Mark.id)
        }
        if let handles {
            let x = Space.xs + CGFloat(CodeStyle.diffCodeStart(gutter, digits: digits)) * DrawnText.advance(size) - 22
            if rows.contains(handles.top) {
                PickHandle(edge: .top, rowHeight: rowHeight, moved: { actions.handleMoved(.top, $0) }, ended: actions.handleEnded)
                    .offset(x: x, y: y(handles.top) - 22)
            }
            if rows.contains(handles.bottom) {
                PickHandle(edge: .bottom, rowHeight: rowHeight, moved: { actions.handleMoved(.bottom, $0) }, ended: actions.handleEnded)
                    .offset(x: x, y: y(handles.bottom) + rowHeight - 22)
            }
        }
    }

    /// The lines this block shows, in order, each once.
    private var shown: [Int] {
        if sideBySide { return rows.flatMap { r in [pairs[r].left, pairs[r].right].compactMap { $0 } } }
        var out: [Int] = []
        for r in rows where out.last != index(r).line { out.append(index(r).line) }
        return out
    }

    private var selection: CGColor { DrawnText.cgColor(Palette.selection.rgba(dark: dark)) }

    private func isPicked(_ i: Int) -> Bool { picked?.contains(i) == true && lines[i].kind != .hunk }

    private func unified(_ row: Int, _ texts: DiffTexts) -> [Cell] {
        let (i, part) = index(row)
        let line = lines[i]
        if line.kind == .hunk { return [Cell(fill: DiffRow.fill(.hunk, dark: dark), text: texts.hunk(line), inset: Space.xs + Space.m)] }
        let cell = Cell(fill: DiffRow.fill(line.kind, dark: dark), text: texts.row(line, gutter, part: part), inset: Space.xs)
        return isPicked(i) ? [Cell(fill: selection), cell] : [cell]
    }

    private func pair(_ pair: SplitRow, _ texts: DiffTexts) -> [Cell] {
        if let i = pair.left, lines[i].kind == .hunk {
            return [Cell(width: column * 2, fill: DiffRow.fill(.hunk, dark: dark), text: texts.hunk(lines[i]), inset: Space.xs + Space.m)]
        }
        func half(_ index: Int?, _ side: CodeStyle.Gutter, x: CGFloat) -> [Cell] {
            guard let index else { return [Cell(x: x, width: column, fill: DrawnText.cgColor(Palette.fill1.rgba(dark: dark)))] }
            let line = lines[index]
            let cell = Cell(x: x, width: column, fill: DiffRow.fill(line.kind, dark: dark), text: texts.row(line, side), inset: Space.xs)
            return isPicked(index) ? [Cell(x: x, width: column, fill: selection), cell] : [cell]
        }
        return half(pair.left, .old, x: 0) + [Cell(x: column, width: 1, fill: DrawnText.cgColor(Palette.separator.rgba(dark: dark)))]
            + half(pair.right, .new, x: column + 1)
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

    func row(_ line: UnifiedLine, _ gutter: CodeStyle.Gutter, part: Range<Int>? = nil) -> DrawnText {
        spans.texts.text(DiffRowKey(row: line.id, part: part, gutter: gutter, dark: dark, size: size), coloured: spans.coloured(line.id)) {
            DrawnText(
                CodeStyle.diffRow(line, gutter: gutter, digits: digits, spans: spans.spans(line.id), dark: dark, part: part), size: size,
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
