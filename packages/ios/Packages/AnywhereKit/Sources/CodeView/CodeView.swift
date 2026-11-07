import DesignSystem
import DiffModel
import Highlight
import SwiftUI
import Wire

/// Read-only source text, virtualized by row, highlighted off the main actor. It can wrap, hide line numbers, change
/// text size, find, open at a line and pick lines to quote.
@MainActor
public struct CodeTextView: View {
    let text: String
    let language: String
    let lineNumbers: Bool
    let wrap: Bool
    let textSize: Int
    let line: Int?
    let onAsk: ((CodeQuote) -> Void)?
    @Binding var finding: Bool
    @State private var loaded: Loaded?
    @State private var laidOut: LaidOut?
    @State private var expanded: Set<Int> = []
    @State private var query = ""
    @State private var find: CodeFind?
    @State private var pick: LinePick?
    @State private var handle: (edge: LinePick.Edge, from: Int)?
    @State private var mark: Mark?
    @State private var flash: Int?
    @State private var shownLine: Int?
    @State private var scrollX: CGFloat = 0
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var baseRow: CGFloat = 18
    @ScaledMetric(relativeTo: .footnote) private var baseSize = DrawnText.footnote

    private struct Loaded {
        let id: Int
        let lines: CodeLines
        let spans: SpanCache
    }

    private struct LaidOut {
        let key: LayoutKey
        let rows: CodeRows
    }

    private struct LayoutKey: Equatable {
        let id: Int
        let columns: Int
        let expanded: Set<Int>
    }

    /// `textSize` adds points to the Dynamic Type size (`CodeStyle.textSizes`). `line` (from 1) is brought to the middle
    /// and flashed once the text is in. `finding` shows the find bar. `onAsk` adds Ask in Chat to picked lines.
    public init(
        text: String, language: String, lineNumbers: Bool = true, wrap: Bool = false, textSize: Int = 0, line: Int? = nil,
        finding: Binding<Bool> = .constant(false), onAsk: ((CodeQuote) -> Void)? = nil
    ) {
        self.text = text
        self.language = language
        self.lineNumbers = lineNumbers
        self.wrap = wrap
        self.textSize = textSize
        self.line = line
        _finding = finding
        self.onAsk = onAsk
    }

    private var size: CGFloat { baseSize + CGFloat(textSize.clamped(to: CodeStyle.textSizes)) }
    private var rowHeight: CGFloat { (baseRow * size / baseSize).rounded() }

    public var body: some View {
        let _ = BodyCount.hit("CodeTextView")
        GeometryReader { geo in
            let key = loaded.flatMap { wrap ? LayoutKey(id: $0.id, columns: columns($0.lines, width: geo.size.width), expanded: expanded) : nil }
            Group {
                if let loaded, let rows = rows(loaded) {
                    content(loaded, rows: rows, width: geo.size.width)
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .task(id: key) { await layOut(key) }
        }
        .background(Palette.surface.color)
        .safeAreaInset(edge: .top, spacing: 0) {
            if finding {
                FindBar(query: $query, position: find?.position ?? "", hasMatches: !(find?.matches.isEmpty ?? true), step: step) {
                    finding = false
                }
            }
        }
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if let pick, let loaded, let quote = CodeQuote.code(loaded.lines.lines, pick: pick.lines, language: language) {
                PickBar(quote: quote, copyText: quote.text, ask: onAsk) { self.pick = nil }
            }
        }
        .task(id: TextKey(text: text, language: language)) { await load() }
        .task(id: FindKey(id: loaded?.id, query: finding ? query : "")) { await search() }
        .task(id: LineKey(line: line, ready: loaded.map { rows($0) != nil } ?? false)) { showLine() }
    }

    @ViewBuilder
    private func content(_ loaded: Loaded, rows: CodeRows, width: CGFloat) -> some View {
        ScrollViewReader { proxy in
            ScrollView(wrap ? .vertical : [.vertical, .horizontal]) {
                LazyVStack(alignment: .leading, spacing: 0) {
                    ForEach(0..<DrawnRows.blocks(rows.count), id: \.self) { b in
                        block(loaded, rows: rows, range: DrawnRows.range(b, count: rows.count), width: width)
                    }
                }
                .padding(.vertical, Space.s)
                .reportsScrollX(in: "code")
            }
            .coordinateSpace(name: "code")
            .onPreferenceChange(ScrollX.self) { x in MainActor.assumeIsolated { scrollX = x } }
            .task(id: mark) {
                guard let mark else { return }
                proxy.scrollTo(mark.row / DrawnRows.block, anchor: UnitPoint(x: 0, y: 0.5))
                // The row's block is built only once scrolled to; its marker can't be reached before.
                try? await Task.sleep(for: .milliseconds(40))
                proxy.scrollTo(Mark.id, anchor: UnitPoint(x: mark.x > 0 ? 0.5 : 0, y: 0.5))
            }
        }
    }

    private func block(_ loaded: Loaded, rows: CodeRows, range: Range<Int>, width: CGFloat) -> CodeBlock {
        let picked = pick?.lines
        return CodeBlock(
            lines: loaded.lines, rows: rows, range: range, digits: lineNumbers ? loaded.lines.digits : nil, minWidth: width,
            wrap: wrap, scrollX: scrollX, rowHeight: rowHeight, size: size, spans: loaded.spans, dark: scheme == .dark, find: find,
            picked: picked, handles: picked.map { handleRows($0, rows: rows) }, flash: flash.map { rows.rows(of: $0) }, mark: mark,
            tapped: { tap(rows[$0]) },
            pressed: { row in
                guard !rows[row].more else { return }
                pick = LinePick(rows[row].line)
                Haptics.tap()
            },
            dragged: { row in _ = pick?.drag(to: rows[row].line) },
            handleMoved: { edge, by in moveHandle(edge, by: by, rows: rows) },
            handleEnded: { handle = nil })
    }

    /// The rows the pick's handles sit on: the top one's first row, the bottom one's last. A handle being dragged
    /// stays on the row it started from and follows the finger by offset.
    private func handleRows(_ picked: ClosedRange<Int>, rows: CodeRows) -> (top: Int, bottom: Int) {
        let top = handle.flatMap { $0.edge == .top ? $0.from : nil } ?? rows.first(of: picked.lowerBound)
        let bottom = handle.flatMap { $0.edge == .bottom ? $0.from : nil } ?? rows.rows(of: picked.upperBound).upperBound - 1
        return (top, bottom)
    }

    private func moveHandle(_ edge: LinePick.Edge, by: Int, rows: CodeRows) {
        guard let picked = pick?.lines else { return }
        let from = handle?.from ?? (edge == .top ? rows.first(of: picked.lowerBound) : rows.rows(of: picked.upperBound).upperBound - 1)
        if handle == nil { handle = (edge, from) }
        let row = (from + by).clamped(to: 0...max(rows.count - 1, 0))
        _ = pick?.move(edge, to: rows[row].line)
    }

    private func tap(_ row: CodeRows.Row) {
        if pick != nil {
            pick = nil
        } else if row.more {
            expanded.insert(row.line)
        }
    }

    private func rows(_ loaded: Loaded) -> CodeRows? {
        guard wrap else { return CodeRows(lineCount: loaded.lines.count) }
        return laidOut.flatMap { $0.key.id == loaded.id ? $0.rows : nil }
    }

    private func columns(_ lines: CodeLines, width: CGFloat) -> Int {
        let gutter = CodeStyle.codeStart(digits: lineNumbers ? lines.digits : nil)
        return max(Int((width - Space.m * 2) / DrawnText.advance(size)) - gutter, 8)
    }

    private func load() async {
        let text = text
        let columns = wrap ? laidOut?.key.columns : nil
        // Lines and their rows land together, so a reload keeps the scroll view and its place.
        let (lines, rows) = await Task.detached(priority: .userInitiated) {
            let lines = CodeLines(text)
            return (lines, columns.map { CodeRows(lines.lines, columns: $0) })
        }.value
        let id = (loaded?.id ?? 0) + 1
        if pick != nil { pick = nil }
        if !expanded.isEmpty { expanded = [] }
        if let rows, let columns { laidOut = LaidOut(key: LayoutKey(id: id, columns: columns, expanded: []), rows: rows) }
        loaded = Loaded(id: id, lines: lines, spans: SpanCache(lines: lines.lines, language: language))
    }

    private func layOut(_ key: LayoutKey?) async {
        guard let key, let loaded, loaded.id == key.id, laidOut?.key != key else { return }
        let lines = loaded.lines.lines
        let rows = await Task.detached(priority: .userInitiated) { CodeRows(lines, columns: key.columns, expanded: key.expanded) }.value
        if !Task.isCancelled { laidOut = LaidOut(key: key, rows: rows) }
    }

    private func search() async {
        guard finding, !query.isEmpty, let loaded else {
            if find != nil { find = nil }
            return
        }
        // Typing replaces the search before it runs.
        try? await Task.sleep(for: .milliseconds(150))
        guard !Task.isCancelled else { return }
        let (query, lines) = (query, loaded.lines.lines)
        let found = await Task.detached(priority: .userInitiated) { CodeFind(query, in: lines) }.value
        guard !Task.isCancelled else { return }
        find = found
        reveal(found.match)
    }

    private func step(_ by: Int) {
        find?.step(by: by)
        reveal(find?.match)
        Haptics.tap()
    }

    private func reveal(_ match: CodeFind.Match?) {
        guard let match, let loaded, let rows = rows(loaded) else { return }
        let all = rows.rows(of: match.line)
        let row = all.first { rows[$0].range.map { $0.contains(match.range.lowerBound) } ?? !rows[$0].more } ?? all.upperBound - 1
        let column = CodeStyle.codeStart(digits: lineNumbers ? loaded.lines.digits : nil) + match.range.lowerBound - (rows[row].range?.lowerBound ?? 0)
        let x = Space.m + CGFloat(column) * DrawnText.advance(size)
        mark = Mark(row: row, x: wrap || x < 240 ? 0 : x, serial: (mark?.serial ?? 0) + 1)
    }

    private func showLine() {
        guard let line, line != shownLine, let loaded, let rows = rows(loaded) else { return }
        shownLine = line
        let index = (line - 1).clamped(to: 0...max(loaded.lines.count - 1, 0))
        mark = Mark(row: rows.first(of: index), x: 0, serial: (mark?.serial ?? 0) + 1)
        flash = index
        Task {
            try? await Task.sleep(for: .seconds(1.2))
            withAnimation(.easeOut(duration: 0.6)) { flash = nil }
        }
    }

    private struct TextKey: Equatable {
        let text: String
        let language: String
    }

    private struct FindKey: Equatable {
        let id: Int?
        let query: String
    }

    private struct LineKey: Equatable {
        let line: Int?
        let ready: Bool
    }
}

/// A row to scroll to the middle, and how far along it (0 for its start); a new `serial` scrolls again.
struct Mark: Equatable {
    static let id = "code-mark"
    let row: Int
    let x: CGFloat
    let serial: Int
}

/// A block of rows, numbered, drawn in one `Canvas` from texts `SpanCache` keeps, with find matches, picked lines and
/// the line flashed after a jump.
@MainActor
private struct CodeBlock: View {
    let lines: CodeLines
    let rows: CodeRows
    let range: Range<Int>
    let digits: Int?
    let minWidth: CGFloat
    let wrap: Bool
    let scrollX: CGFloat
    let rowHeight: CGFloat
    let size: CGFloat
    let spans: SpanCache
    let dark: Bool
    let find: CodeFind?
    let picked: ClosedRange<Int>?
    let handles: (top: Int, bottom: Int)?
    let flash: Range<Int>?
    let mark: Mark?
    let tapped: (Int) -> Void
    let pressed: (Int) -> Void
    let dragged: (Int) -> Void
    let handleMoved: (LinePick.Edge, Int) -> Void
    let handleEnded: () -> Void

    var body: some View {
        let _ = BodyCount.hit("CodeBlock")
        let cells = range.map(cells)
        let width = wrap ? minWidth : cells.reduce(minWidth) { max($0, ($1.text?.width ?? 0) + Space.m * 2) }
        DrawnRows(rows: cells.map { [$0] }, rowHeight: rowHeight, width: width, window: wrap ? nil : (scrollX, minWidth))
            .overlay(alignment: .topLeading) { marks }
            .contentShape(Rectangle())
            .onTapGesture(coordinateSpace: .local) { tapped(row(at: $0)) }
            .onLinePress(began: { pressed(row(at: $0)) }, moved: { dragged(row(at: $0)) }, ended: {})
            .onAppear { range.forEach { spans.need(rows[$0].line) } }
            .accessibilityElement(children: .contain)
            .accessibilityChildren {
                ForEach(shownLines, id: \.self) { Text(lines.lines[$0]) }
            }
    }

    private func row(at point: CGPoint) -> Int {
        (range.lowerBound + Int((point.y / rowHeight).rounded(.down))).clamped(to: 0...max(rows.count - 1, 0))
    }

    private func y(_ row: Int) -> CGFloat { CGFloat(row - range.lowerBound) * rowHeight }

    @ViewBuilder
    private var marks: some View {
        if let flash, flash.overlaps(range) {
            let shown = flash.clamped(to: range)
            Rectangle()
                .fill(CodeStyle.match.color)
                .frame(height: CGFloat(shown.count) * rowHeight)
                .offset(y: y(shown.lowerBound))
                .allowsHitTesting(false)
                .transition(.opacity)
        }
        if let mark, range.contains(mark.row) {
            Color.clear.frame(width: 1, height: rowHeight).offset(x: mark.x, y: y(mark.row)).id(Mark.id)
        }
        if let handles {
            let x = Space.m + CGFloat(CodeStyle.codeStart(digits: digits)) * DrawnText.advance(size) - 22
            if range.contains(handles.top) {
                PickHandle(edge: .top, rowHeight: rowHeight, moved: { handleMoved(.top, $0) }, ended: handleEnded)
                    .offset(x: x, y: y(handles.top) - 22)
            }
            if range.contains(handles.bottom) {
                PickHandle(edge: .bottom, rowHeight: rowHeight, moved: { handleMoved(.bottom, $0) }, ended: handleEnded)
                    .offset(x: x, y: y(handles.bottom) + rowHeight - 22)
            }
        }
    }

    /// The lines this block shows, each once.
    private var shownLines: [Int] {
        var out: [Int] = []
        for i in range where out.last != rows[i].line { out.append(rows[i].line) }
        return out
    }

    private func cells(_ i: Int) -> Cell {
        let row = rows[i]
        let fill = picked.map { $0.contains(row.line) } == true ? DrawnText.cgColor(Palette.selection.rgba(dark: dark)) : nil
        guard !row.more else {
            let text = spans.texts.text(RowKey(line: row.line, part: -1..<0, numbers: digits != nil, dark: dark, size: size), coloured: true) {
                var runs = LineRuns()
                runs.append(String(repeating: " ", count: CodeStyle.codeStart(digits: digits)))
                runs.append("Show full line", rgba: Palette.link.rgba(dark: dark))
                return DrawnText(runs, size: size, color: Palette.link.rgba(dark: dark))
            }
            return Cell(fill: fill, text: text, inset: Space.m)
        }
        let source = lines.lines[row.line]
        var part = row.range
        var cut = false
        if part == nil, source.utf8.count > CodeStyle.maxChars {
            let c = CodeRows.cut(source, columns: .max, limit: CodeStyle.maxChars)
            part = c.ranges.first
            cut = c.more
        }
        let key = RowKey(line: row.line, part: row.range ?? 0..<Int.max, numbers: digits != nil, dark: dark, size: size)
        let text = spans.texts.text(key, coloured: spans.coloured(row.line)) {
            DrawnText(
                CodeStyle.codeRow(
                    source, number: row.first ? row.line + 1 : nil, digits: digits, spans: spans.spans(row.line), dark: dark, part: part, cut: cut),
                size: size, color: Palette.text.rgba(dark: dark))
        }
        return Cell(fill: fill, text: text, inset: Space.m, marks: marks(row, part: part))
    }

    /// Find matches inside the row, moved past its gutter; the current one stronger.
    private func marks(_ row: CodeRows.Row, part: Range<Int>?) -> [Cell.Mark] {
        guard let find else { return [] }
        let ranges = find.ranges(in: row.line)
        guard !ranges.isEmpty else { return [] }
        let start = CodeStyle.codeStart(digits: digits)
        let shown = part ?? 0..<Int.max
        let current = find.match.flatMap { $0.line == row.line ? $0.range : nil }
        return ranges.overlapping(shown, \.self).compactMap { r in
            let lo = max(r.lowerBound, shown.lowerBound)
            let hi = min(r.upperBound, shown.upperBound)
            guard lo < hi else { return nil }
            let token = r == current ? CodeStyle.currentMatch : CodeStyle.match
            return Cell.Mark(range: (start + lo - shown.lowerBound)..<(start + hi - shown.lowerBound), color: DrawnText.cgColor(token.rgba(dark: dark)))
        }
    }
}

extension Comparable {
    func clamped(to r: ClosedRange<Self>) -> Self { min(max(self, r.lowerBound), r.upperBound) }
}
