import DesignSystem
import Highlight
import MarkdownModel
import SwiftUI
#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// Code-block highlights by row, for the code length and appearance they were computed for
/// (desktop `pocket/chat/markdown.rs Highlights`). Code that changed length (a block still streaming) keeps the old
/// run's colours, cut to fit, until its own run lands.
struct CodeColours {
    private var rows: [String: (length: Int, dark: Bool, spans: [HighlightSpan]?)] = [:]

    /// The spans for a row's `code` in a light or dark appearance, and whether it needs a new run.
    mutating func get(_ row: String, code: String, dark: Bool) -> (spans: [HighlightSpan]?, needsRun: Bool) {
        let length = code.utf16.count
        var old: [HighlightSpan]?
        if let entry = rows[row] {
            if entry.length == length, entry.dark == dark { return (entry.spans, false) }
            if entry.dark == dark, let spans = entry.spans { old = Self.kept(spans, code) }
        }
        rows[row] = (length, dark, old)
        return (old, true)
    }

    /// A run landing for `length` and `dark`; dropped unless it's still what the row shows.
    mutating func set(_ row: String, length: Int, dark: Bool, spans: [HighlightSpan]) {
        guard let entry = rows[row], entry.length == length, entry.dark == dark else { return }
        rows[row] = (length, dark, spans)
    }

    /// The spans of a run made for other code that still fit `code`: cut at its end, dropping any that would split a character.
    static func kept(_ spans: [HighlightSpan], _ code: String) -> [HighlightSpan] {
        let units = Array(code.utf16)
        func boundary(_ k: Int) -> Bool { k == 0 || k == units.count || (k < units.count && !UTF16.isTrailSurrogate(units[k])) }
        return spans.compactMap { span in
            let end = min(span.range.upperBound, units.count)
            guard span.range.lowerBound < end, boundary(span.range.lowerBound), boundary(end) else { return nil }
            return HighlightSpan(range: span.range.lowerBound..<end, kind: span.kind)
        }
    }
}

/// Highlight runs shared by every code block, by language and code, so a block shown again costs nothing.
final class HighlightCache: @unchecked Sendable {
    static let shared = HighlightCache()
    private let lock = NSLock()
    private var spans: [String: [HighlightSpan]] = [:]
    private var order: [String] = []

    func spans(_ code: String, language: String) -> [HighlightSpan] {
        let key = "\(language)\u{0}\(Markdown.digest(code))\u{0}\(code.utf8.count)"
        if let hit = lock.withLock({ spans[key] }) { return hit }
        let found = Highlighter.spans(code, language: language)
        lock.withLock {
            if spans.updateValue(found, forKey: key) == nil { order.append(key) }
            if order.count > 256 { spans[order.removeFirst()] = nil }
        }
        return found
    }
}

/// `code` coloured by `spans` in the light or dark palette.
func colouredCode(_ code: String, _ spans: [HighlightSpan], dark: Bool) -> AttributedString {
    var out = AttributedString(code)
    guard !spans.isEmpty else { return out }
    let utf16 = code.utf16
    for span in spans {
        guard let rgb = SyntaxPalette.rgb(span.kind, dark: dark), span.range.upperBound <= utf16.count else { continue }
        let lower = String.Index(utf16Offset: span.range.lowerBound, in: code)
        let upper = String.Index(utf16Offset: span.range.upperBound, in: code)
        guard let range = Range(lower..<upper, in: out) else { continue }
        out[range].foregroundColor = Token.fixed(rgb << 8 | 0xff).color
    }
    return out
}

/// A fenced code block as the chat draws it: language label, wrap toggle, copy, highlight.
@MainActor
public struct CodeBlockView: View {
    let code: String
    let language: String?
    let label: String
    let size: CGFloat
    let line: CGFloat

    @Environment(\.colorScheme) private var colorScheme
    @State private var colours = CodeColours()
    @State private var styled: AttributedString?
    @State private var styledFor: (length: Int, dark: Bool)?
    @State private var wrapped = false
    @State private var copied = false

    public init(code: String, language: String?) {
        self.init(code: code, language: language, label: language?.lowercased() ?? "code", size: 12.5, line: 18)
    }

    init(code: String, language: String?, label: String, size: CGFloat, line: CGFloat) {
        self.code = code
        self.language = language
        self.label = label
        self.size = size
        self.line = line
    }

    private var dark: Bool { colorScheme == .dark }

    public var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Rectangle().fill(Palette.separator.color).frame(height: 1)
            content
        }
        .background(Palette.fill1.color, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(Palette.separator.color, lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
        .task(id: RunKey(code: code, language: language, dark: dark)) { await highlight() }
    }

    private var header: some View {
        HStack(spacing: 2) {
            Text(label)
                .font(.system(size: 11))
                .foregroundStyle(Palette.text2.color)
                .lineLimit(1)
            Spacer(minLength: 8)
            Button {
                wrapped.toggle()
                Haptics.tap()
            } label: {
                Image(systemName: "text.alignleft")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle((wrapped ? Palette.text : Palette.text2).color)
                    .frame(width: 28, height: 28)
                    .background(wrapped ? Palette.fill3.color : .clear, in: RoundedRectangle(cornerRadius: 5))
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help(wrapped ? "Scroll long lines" : "Wrap long lines")
            .accessibilityLabel(wrapped ? "Scroll long lines" : "Wrap long lines")
            Button(action: copy) {
                HStack(spacing: 4) {
                    Image(systemName: copied ? "checkmark" : "doc.on.doc").font(.system(size: 11, weight: .medium))
                    if copied { Text("Copied").font(.system(size: 10.5)) }
                }
                .foregroundStyle(Palette.text2.color)
                .padding(.horizontal, 6)
                .frame(minWidth: 28, minHeight: 28)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .help("Copy code")
            .accessibilityLabel(copied ? "Copied" : "Copy code")
        }
        .padding(.leading, 12)
        .padding(.trailing, 2)
        .frame(height: 28)
        .background(Palette.fill1.color)
    }

    @ViewBuilder
    private var content: some View {
        let text = Text(currentStyled)
            .font(.system(size: size, design: .monospaced))
            .foregroundStyle(Palette.text.color)
            .lineSpacing(Metrics.spacing(size: size, line: line))
            .textSelection(.enabled)
            .padding(.horizontal, 12)
            .padding(.vertical, 10)
        if wrapped {
            text.fixedSize(horizontal: false, vertical: true).frame(maxWidth: .infinity, alignment: .leading)
        } else {
            ScrollView(.horizontal, showsIndicators: false) {
                text.fixedSize(horizontal: true, vertical: true)
            }
            .scrollBounceBehavior(.basedOnSize, axes: .horizontal)
        }
    }

    /// The colours for this code if they've landed, else the last run's cut to fit, else none.
    private var currentStyled: AttributedString {
        if let styled, let styledFor, styledFor.dark == dark {
            if styledFor.length == code.utf16.count { return styled }
            let (spans, _) = peek()
            return colouredCode(code, spans ?? [], dark: dark)
        }
        return AttributedString(code)
    }

    private func peek() -> (spans: [HighlightSpan]?, needsRun: Bool) {
        var copy = colours
        return copy.get("code", code: code, dark: dark)
    }

    private func highlight() async {
        let (code, dark) = (self.code, self.dark)
        let length = code.utf16.count
        _ = colours.get("code", code: code, dark: dark)
        guard let language, Highlighter.language(for: language) != "text" else {
            styled = AttributedString(code)
            styledFor = (length, dark)
            return
        }
        let result = await Task.detached(priority: .userInitiated) { () -> ([HighlightSpan], AttributedString) in
            let spans = HighlightCache.shared.spans(code, language: language)
            return (spans, colouredCode(code, spans, dark: dark))
        }.value
        guard !Task.isCancelled else { return }
        colours.set("code", length: length, dark: dark, spans: result.0)
        styled = result.1
        styledFor = (length, dark)
    }

    private func copy() {
        #if os(iOS)
            UIPasteboard.general.string = code
        #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(code, forType: .string)
        #endif
        Haptics.success()
        copied = true
        Task {
            try? await Task.sleep(for: .seconds(1.2))
            copied = false
        }
    }

    struct RunKey: Equatable {
        let code: String
        let language: String?
        let dark: Bool
    }
}
