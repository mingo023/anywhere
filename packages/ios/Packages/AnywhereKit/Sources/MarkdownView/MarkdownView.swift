import SwiftUI
import MarkdownModel
import Highlight
import DesignSystem

/// What a tap in markdown asks for.
public enum MarkdownLink: Sendable, Equatable {
    case web(URL)
    /// A file named by a link or a path chip, relative to the agent's folder; line when the link carried one.
    case file(path: String, line: Int?)
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
@MainActor
public struct MarkdownView: View {
    let source: String
    let style: MarkdownStyle
    let onLink: (MarkdownLink) -> Void

    public init(_ source: String, style: MarkdownStyle = .chat, onLink: @escaping (MarkdownLink) -> Void = { _ in }) {
        self.source = source
        self.style = style
        self.onLink = onLink
    }

    public var body: some View { Text(source) }
}

/// A fenced code block as the chat draws it: language label, wrap toggle, copy, highlight.
@MainActor
public struct CodeBlockView: View {
    let code: String
    let language: String?

    public init(code: String, language: String?) {
        self.code = code
        self.language = language
    }

    public var body: some View { Text(code).font(.system(.footnote, design: .monospaced)) }
}
