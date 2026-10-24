import SwiftUI
import MarkdownModel
import Highlight
import DesignSystem

/// What a tap in markdown asks for.
public enum MarkdownLink: Sendable, Equatable {
    case web(URL)
    /// A file named by a link or a path chip, relative to the agent's folder; line when the link carried one.
    case file(path: String, line: Int?)

    /// Where a link points (desktop `pocket/chat/markdown.rs target`): a `://` URL that isn't `file://` (or a
    /// `mailto:`/`tel:`) opens on the web; anything else is a file, without its `:line:col` or `#L…` suffix.
    /// nil for a link to nowhere (`#heading`, empty).
    public nonisolated static func target(_ link: String) -> MarkdownLink? {
        let lower = link.lowercased()
        if (link.contains("://") && !lower.hasPrefix("file://")) || lower.hasPrefix("mailto:") || lower.hasPrefix("tel:") {
            return URL(string: link).map { .web($0) }
        }
        var rest = Substring(link)
        if lower.hasPrefix("file://") { rest = rest.dropFirst(7) }
        let fragment = rest.firstIndex(of: "#").map { rest[rest.index(after: $0)...] }
        var path = rest.split(separator: "#", maxSplits: 1, omittingEmptySubsequences: false).first ?? ""
        var numbers: [Int] = []
        for _ in 0..<2 {
            guard let colon = path.lastIndex(of: ":"), case let digits = path[path.index(after: colon)...], !digits.isEmpty, digits.allSatisfy({ $0.isASCII && $0.isNumber }), let n = Int(digits) else { break }
            numbers.append(n)
            path = path[..<colon]
        }
        guard !path.isEmpty else { return nil }
        let decoded = String(path).removingPercentEncoding ?? String(path)
        var line = numbers.last
        if let fragment, fragment.hasPrefix("L"), let n = Int(fragment.dropFirst().prefix(while: \.isNumber)) { line = n }
        return .file(path: decoded, line: line)
    }
}

public enum MarkdownStyle: Sendable {
    /// Chat body: 14/24 text.
    case chat
    /// Smaller text for sub-agent reports and question bodies.
    case compact
    /// File preview and PR descriptions.
    case document
}

/// Renders a markdown string. Parsing is cached by content; a growing string re-parses only what changed.
/// Short text (or a short growth of text seen lately) parses on the spot; longer text parses off the main actor while
/// the last version shows.
@MainActor
public struct MarkdownView: View {
    let source: String
    let style: MarkdownStyle
    let onLink: (MarkdownLink) -> Void

    @State private var parsed: MarkdownDocument?
    @Environment(\.dynamicTypeSize) private var typeSize

    public init(_ source: String, style: MarkdownStyle = .chat, onLink: @escaping (MarkdownLink) -> Void = { _ in }) {
        self.source = source
        self.style = style
        self.onLink = onLink
    }

    public var body: some View {
        let now = MarkdownCache.shared.documentIfCheap(source)
        let document = now ?? parsed
        let metrics = Metrics.of(style, scale: Metrics.scale(typeSize))
        Group {
            if let document {
                blocks(document, metrics)
            } else {
                // Until a long document lands, its source reads as text.
                Text(source.prefix(4000))
                    .font(.system(size: metrics.body))
                    .foregroundStyle(Palette.textBody.color)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .redacted(reason: .placeholder)
            }
        }
        .environment(\.openURL, OpenURLAction { url in
            if let destination = LinkURL.destination(of: url) {
                if let link = MarkdownLink.target(destination) { onLink(link) }
                return .handled
            }
            return .systemAction
        })
        .task(id: now == nil ? source : "") {
            guard now == nil else { return }
            let source = self.source
            let document = await Task.detached(priority: .userInitiated) { MarkdownCache.shared.document(source) }.value
            if !Task.isCancelled { parsed = document }
        }
    }

    @ViewBuilder
    private func blocks(_ document: MarkdownDocument, _ metrics: Metrics) -> some View {
        if style == .document {
            LazyVStack(alignment: .leading, spacing: metrics.gap) {
                ForEach(document.blocks) { block in TopBlockView(block: block, metrics: metrics).equatable() }
            }
        } else {
            VStack(alignment: .leading, spacing: metrics.gap) {
                ForEach(document.blocks) { block in TopBlockView(block: block, metrics: metrics).equatable() }
            }
        }
    }
}
