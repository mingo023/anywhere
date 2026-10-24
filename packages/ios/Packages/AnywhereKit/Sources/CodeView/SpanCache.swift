import Highlight
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
