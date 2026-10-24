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
    @Environment(\.colorScheme) private var scheme
    @ScaledMetric(relativeTo: .footnote) private var rowHeight: CGFloat = 18
    @ScaledMetric(relativeTo: .footnote) private var charWidth: CGFloat = 7.8

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
                        ForEach(0..<loaded.lines.count, id: \.self) { i in
                            CodeRow(
                                number: lineNumbers ? i + 1 : nil, gutter: CGFloat(loaded.lines.digits) * charWidth, text: loaded.lines.lines[i],
                                line: i, spans: loaded.spans, dark: scheme == .dark
                            )
                            .frame(height: rowHeight)
                            .frame(minWidth: geo.size.width, alignment: .leading)
                        }
                    }
                    .padding(.vertical, Space.s)
                }
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

@MainActor
private struct CodeRow: View {
    let number: Int?
    let gutter: CGFloat
    let text: String
    let line: Int
    let spans: SpanCache
    let dark: Bool

    var body: some View {
        let _ = BodyCount.hit("CodeRow")
        HStack(spacing: Space.m) {
            if number != nil { LineNumber(number: number, width: gutter) }
            Text(CodeStyle.attributed(text, spans: spans.spans(line), dark: dark))
                .font(CodeStyle.font)
                .foregroundStyle(Palette.text.color)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, Space.m)
        .onAppear { spans.need(line) }
    }
}
