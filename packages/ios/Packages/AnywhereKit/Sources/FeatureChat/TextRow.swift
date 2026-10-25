import ChatModel
import DesignSystem
import MarkdownView
import SwiftUI

/// One markdown block of the agent's text, let out word by word while it streams; a plan's first block gets a header.
@MainActor
struct TextRow: View {
    let text: TextContent
    let reveal: (_ message: String) -> Reveal?
    let onLink: (MarkdownLink) -> Void

    var body: some View {
        let _ = BodyCount.hit("TextRow")
        VStack(alignment: .leading, spacing: 6) {
            if text.planHeader {
                Label("Plan", systemImage: "checklist").font(ChatFont.small.weight(.medium)).foregroundStyle(Palette.text2.color)
            }
            if text.revealing {
                RevealText(text: text, reveal: reveal, onLink: onLink)
            } else {
                MarkdownView(text.text, style: .chat, onLink: onLink)
            }
        }
    }
}

/// A block being let out: only it reads the reveal, which moves with every chunk.
@MainActor
private struct RevealText: View {
    let text: TextContent
    let reveal: (_ message: String) -> Reveal?
    let onLink: (MarkdownLink) -> Void

    var body: some View {
        if let reveal = reveal(text.message) {
            TimelineView(.animation(minimumInterval: 1.0 / 30, paused: !reveal.moving(Date.timeIntervalSinceReferenceDate))) { context in
                let cut = reveal.cut(text.text, from: text.offset, context.date.timeIntervalSinceReferenceDate)
                MarkdownView(String(decoding: text.text.utf8.prefix(cut), as: UTF8.self), style: .chat, onLink: onLink)
            }
        } else {
            MarkdownView(text.text, style: .chat, onLink: onLink)
        }
    }
}
