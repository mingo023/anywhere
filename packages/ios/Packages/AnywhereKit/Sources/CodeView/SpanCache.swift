import DiffModel
import Highlight
import CoreGraphics
import Observation

/// One window's colours, observed only by the rows inside it.
@MainActor
@Observable
final class SpanWindow {
    var spans: [[HighlightSpan]]?
}

/// Syntax colours for a long list of lines, filled a window at a time off the main actor as rows come on screen.
@MainActor
final class SpanCache {
    let language: String
    private let lines: [String]
    private let windows: [SpanWindow]
    private var started = Set<Int>()
    /// Each line's text, by line and appearance.
    let texts = RowTexts<RowKey>()

    init(lines: [String], language: String) {
        self.lines = lines
        self.language = language
        windows = (0..<(lines.count + HighlightWindow.size - 1) / HighlightWindow.size).map { _ in SpanWindow() }
    }

    /// The line's spans once its window is done; empty until then, so text shows plain first.
    func spans(_ line: Int) -> [HighlightSpan] {
        let w = HighlightWindow.window(of: line)
        guard w < windows.count, let spans = windows[w].spans else { return [] }
        return spans[line - w * HighlightWindow.size]
    }

    /// Whether the line's colours are in, or it has none to wait for.
    func coloured(_ line: Int) -> Bool {
        let w = HighlightWindow.window(of: line)
        return language == "text" || w >= windows.count || windows[w].spans != nil
    }

    /// Starts colouring the line's window, once.
    func need(_ line: Int) {
        let w = HighlightWindow.window(of: line)
        guard language != "text", w < windows.count, started.insert(w).inserted else { return }
        let slice = lines[HighlightWindow.range(w, count: lines.count)]
        let language = language
        let window = windows[w]
        Task {
            window.spans = await Task.detached(priority: .userInitiated) { HighlightWindow.spans(slice, language: language) }.value
        }
    }
}

/// A diff's colours: each side's lines highlighted as their own text, so a deletion and its replacement never parse as
/// one program and hunk headers aren't code.
@MainActor
final class DiffSpans {
    private let old: SpanCache
    private let new: SpanCache
    private let kinds: [UnifiedLine.Kind]
    private let at: [Int?]

    init(_ lines: [UnifiedLine], language: String) {
        let sides = DiffRows.sides(lines)
        old = SpanCache(lines: sides.old, language: language)
        new = SpanCache(lines: sides.new, language: language)
        kinds = lines.map(\.kind)
        at = sides.at
    }

    private func side(_ row: Int) -> (SpanCache, Int)? {
        guard at.indices.contains(row), let line = at[row] else { return nil }
        return (kinds[row] == .deleted ? old : new, line)
    }

    func spans(_ row: Int) -> [HighlightSpan] {
        guard let (cache, line) = side(row) else { return [] }
        return cache.spans(line)
    }

    func coloured(_ row: Int) -> Bool {
        guard let (cache, line) = side(row) else { return true }
        return cache.coloured(line)
    }

    /// Each row's text, by row, gutter and appearance.
    let texts = RowTexts<DiffRowKey>()

    func need(_ row: Int) {
        guard let (cache, line) = side(row) else { return }
        cache.need(line)
    }
}

struct RowKey: Hashable {
    let line: Int
    let dark: Bool
    let size: CGFloat
}

struct DiffRowKey: Hashable {
    let row: Int
    let gutter: CodeStyle.Gutter
    let dark: Bool
    let size: CGFloat
}
