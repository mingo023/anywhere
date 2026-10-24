import DesignSystem
import DiffModel
import Highlight
import SwiftUI
import Wire

/// Read-only source text, virtualized by line, highlighted off the main actor.
@MainActor
public struct CodeTextView: View {
    let text: String
    let language: String
    let lineNumbers: Bool
    @State private var loaded: Loaded?
    @State private var scrollX: CGFloat = 0
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 18

    private struct Loaded {
        let lines: CodeLines
        let spans: SpanCache
    }

    public init(text: String, language: String, lineNumbers: Bool = true) {
        self.text = text
        self.language = language
        self.lineNumbers = lineNumbers
    }

    public var body: some View {
        let _ = BodyCount.hit("CodeTextView")
        GeometryReader { geo in
            if let loaded {
                ScrollView([.vertical, .horizontal]) {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(0..<DrawnRows.blocks(loaded.lines.count), id: \.self) { b in
                            CodeBlock(
                                lines: loaded.lines, rows: DrawnRows.range(b, count: loaded.lines.count), numbers: lineNumbers,
                                minWidth: geo.size.width, scrollX: scrollX, rowHeight: rowHeight, spans: loaded.spans, dark: scheme == .dark)
                        }
                    }
                    .padding(.vertical, Space.s)
                    .reportsScrollX(in: "code")
                }
                .coordinateSpace(name: "code")
                .onPreferenceChange(ScrollX.self) { x in MainActor.assumeIsolated { scrollX = x } }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .background(Palette.surface.color)
        .task(id: TextKey(text: text, language: language)) {
            let text = text
            let lines = await Task.detached(priority: .userInitiated) { CodeLines(text) }.value
            loaded = Loaded(lines: lines, spans: SpanCache(lines: lines.lines, language: language))
        }
    }

    private struct TextKey: Equatable {
        let text: String
        let language: String
    }
}

/// A block of source lines, numbered, drawn in one `Canvas` from texts `SpanCache` keeps.
@MainActor
private struct CodeBlock: View {
    let lines: CodeLines
    let rows: Range<Int>
    let numbers: Bool
    let minWidth: CGFloat
    let scrollX: CGFloat
    let rowHeight: CGFloat
    let spans: SpanCache
    let dark: Bool
    @ScaledMetric(relativeTo: .footnote) private var size = DrawnText.footnote

    var body: some View {
        let _ = BodyCount.hit("CodeBlock")
        let texts = rows.map { line in
            spans.texts.text(RowKey(line: line, dark: dark, size: size), coloured: spans.coloured(line)) {
                DrawnText(
                    CodeStyle.codeRow(lines.lines[line], number: numbers ? line + 1 : nil, digits: lines.digits, spans: spans.spans(line), dark: dark),
                    size: size, color: Palette.text.rgba(dark: dark))
            }
        }
        DrawnRows(
            rows: texts.map { [Cell(text: $0, inset: Space.m)] }, rowHeight: rowHeight, width: texts.reduce(minWidth) { max($0, $1.width + Space.m * 2) },
            window: (scrollX, minWidth))
            .onAppear { rows.forEach(spans.need) }
            .accessibilityElement(children: .contain)
            .accessibilityChildren {
                ForEach(rows, id: \.self) { Text(lines.lines[$0]) }
            }
    }
}
