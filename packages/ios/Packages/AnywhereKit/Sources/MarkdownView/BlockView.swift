import DesignSystem
import MarkdownModel
import SwiftUI

/// One top-level block. Equal blocks (same place, same content) skip their body, so a streaming message redraws only
/// its last block.
@MainActor
struct TopBlockView: View, Equatable {
    let block: MarkdownBlock
    let metrics: Metrics

    nonisolated static func == (a: TopBlockView, b: TopBlockView) -> Bool { a.block == b.block && a.metrics == b.metrics }

    var body: some View { BlockView(block: block.block, metrics: metrics, muted: false) }
}

@MainActor
struct BlockView: View {
    let block: MDBlock
    let metrics: Metrics
    /// Inside a ticked task: muted and struck through.
    let muted: Bool

    var body: some View {
        let _ = BodyCount.hit("BlockView")
        switch block {
        case .paragraph(let inlines):
            ParagraphView(inlines: inlines, style: InlineStyle(size: metrics.body, muted: muted), line: metrics.line)
        case .heading(let level, let inlines):
            let h = metrics.heading(level)
            ParagraphView(inlines: inlines, style: InlineStyle(size: h.size, weight: .semibold, color: Palette.text, muted: muted), line: h.line)
                .accessibilityAddTraits(.isHeader)
        case .list(let list):
            ListView(list: list, metrics: metrics, muted: muted)
        case .quote(let blocks):
            QuoteView(blocks: blocks, metrics: metrics)
        case .alert(let alert):
            AlertView(alert: alert, metrics: metrics)
        case .code(let code):
            CodeBlockView(code: code.code, language: code.language, label: code.label, size: metrics.codeSize, line: metrics.codeLine)
        case .table(let table):
            TableView(table: table, metrics: metrics)
        case .rule:
            Rectangle().fill(Palette.separator.color).frame(height: 1).frame(maxWidth: .infinity).padding(.vertical, 4)
        case .footnotes(let notes):
            FootnotesView(notes: notes, metrics: metrics)
        }
    }
}

/// Blocks one under another, `gap` apart.
@MainActor
struct BlocksView: View {
    let blocks: [MDBlock]
    let metrics: Metrics
    var gap: CGFloat?
    var muted = false

    var body: some View {
        VStack(alignment: .leading, spacing: gap ?? metrics.gap) {
            ForEach(blocks.indices, id: \.self) { i in
                BlockView(block: blocks[i], metrics: metrics, muted: muted)
            }
        }
    }
}

/// A paragraph: its text as one selectable `Text`, with any images it holds below their text.
@MainActor
struct ParagraphView: View {
    let inlines: [MDInline]
    let style: InlineStyle
    let line: CGFloat

    enum Piece {
        case text([MDInline])
        case images([LinkedImage])
    }

    /// The paragraph cut where images stand: a run of images (with only spaces between) becomes one row of them.
    nonisolated static func pieces(_ inlines: [MDInline]) -> [Piece] {
        var out: [Piece] = []
        var text: [MDInline] = []
        var images: [LinkedImage] = []
        func isBlank(_ i: MDInline) -> Bool {
            switch i {
            case .softBreak, .hardBreak: true
            case .text(let s): s.allSatisfy(\.isWhitespace)
            default: false
            }
        }
        for inline in inlines {
            var image: LinkedImage?
            if case .image(let i) = inline { image = LinkedImage(image: i, link: nil) }
            if case .link(let destination, _, let content) = inline, content.count == 1, case .image(let i)? = content.first {
                image = LinkedImage(image: i, link: destination)
            }
            if let image {
                if !text.allSatisfy(isBlank) { out.append(.text(text)) }
                text = []
                images.append(image)
            } else if !images.isEmpty, isBlank(inline) {
                continue
            } else {
                if !images.isEmpty { out.append(.images(images)) }
                images = []
                text.append(inline)
            }
        }
        if !images.isEmpty { out.append(.images(images)) }
        if !text.allSatisfy(isBlank) || out.isEmpty { out.append(.text(text)) }
        return out
    }

    var body: some View {
        let pieces = Self.pieces(inlines)
        if pieces.count == 1, case .text(let content) = pieces[0] {
            text(content)
        } else {
            VStack(alignment: .leading, spacing: 8) {
                ForEach(pieces.indices, id: \.self) { i in
                    switch pieces[i] {
                    case .text(let content): text(content)
                    case .images(let images): ImageRow(images: images, height: line)
                    }
                }
            }
        }
    }

    func text(_ content: [MDInline]) -> some View {
        let spacing = Metrics.spacing(size: style.size, line: line)
        return InlineText.text(trimmed(content), style: style)
            .lineSpacing(spacing)
            .padding(.vertical, spacing / 2)
            .fixedSize(horizontal: false, vertical: true)
            .frame(maxWidth: .infinity, alignment: .leading)
            .selectableOnMac()
    }

    /// Without breaks at either end, which an image cut out of the run leaves behind.
    func trimmed(_ content: [MDInline]) -> [MDInline] {
        var c = content[...]
        while let f = c.first, f == .softBreak || f == .hardBreak { c = c.dropFirst() }
        while let l = c.last, l == .softBreak || l == .hardBreak { c = c.dropLast() }
        return Array(c)
    }
}

/// A list: accent dots or right-aligned numbers in the indent, task boxes, nesting.
@MainActor
struct ListView: View {
    let list: MDList
    let metrics: Metrics
    let muted: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: list.tight ? metrics.itemGap : metrics.gap) {
            ForEach(list.items.indices, id: \.self) { i in
                let item = list.items[i]
                HStack(alignment: .firstTextBaseline, spacing: 0) {
                    marker(i, item)
                        .frame(width: metrics.indent, alignment: list.ordered && item.checked == nil ? .trailing : .leading)
                    BlocksView(blocks: item.blocks, metrics: metrics, gap: metrics.itemGap, muted: muted || item.checked == true)
                        .padding(.leading, list.ordered && item.checked == nil ? 6 : 0)
                }
            }
        }
    }

    @ViewBuilder
    func marker(_ i: Int, _ item: MDListItem) -> some View {
        let accent = Palette.accent.color
        if let checked = item.checked {
            TaskBox(checked: checked, size: metrics.body * 0.9)
                .alignmentGuide(.firstTextBaseline) { d in d[.bottom] - metrics.body * 0.12 }
                .accessibilityLabel(checked ? "Done" : "To do")
        } else if list.ordered {
            Text("\(list.start + i).")
                .font(.system(size: metrics.body).monospacedDigit())
                .foregroundStyle(accent)
        } else {
            Circle()
                .fill(accent)
                .frame(width: 5, height: 5)
                .padding(.leading, metrics.indent * 0.3)
                .alignmentGuide(.firstTextBaseline) { d in d[.bottom] + metrics.body * 0.3 - 2.5 }
                .accessibilityHidden(true)
        }
    }
}

/// A task's box: 0.9 em, radius 0.225 em, a 1.5 pt border; ticked, it fills with the link colour around a check.
@MainActor
struct TaskBox: View {
    let checked: Bool
    let size: CGFloat

    var body: some View {
        let shape = RoundedRectangle(cornerRadius: size * 0.25, style: .continuous)
        ZStack {
            if checked {
                shape.fill(Palette.link.color)
                Image(systemName: "checkmark")
                    .font(.system(size: size * 0.62, weight: .bold))
                    .foregroundStyle(.white)
            } else {
                shape.strokeBorder(Palette.text4.color, lineWidth: 1.5)
            }
        }
        .frame(width: size, height: size)
    }
}

/// A quote: a 2 pt accent rule at 60 % over a 5 % accent wash, its right corners rounded.
@MainActor
struct QuoteView: View {
    let blocks: [MDBlock]
    let metrics: Metrics

    var body: some View {
        BlocksView(blocks: blocks, metrics: metrics, gap: metrics.gap * 0.5)
            .padding(.leading, 12)
            .padding(.trailing, 10)
            .padding(.vertical, 6)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(alignment: .leading) {
                ZStack(alignment: .leading) {
                    UnevenRoundedRectangle(topLeadingRadius: 0, bottomLeadingRadius: 0, bottomTrailingRadius: 6, topTrailingRadius: 6, style: .continuous)
                        .fill(Palette.accent.color.opacity(0.05))
                    Rectangle().fill(Palette.accent.color.opacity(0.6)).frame(width: 2)
                }
            }
    }
}

/// A GitHub alert: a 4 pt rule and icon in its kind's colour, its title, then its body, on the aside background.
@MainActor
struct AlertView: View {
    let alert: MDAlert
    let metrics: Metrics

    nonisolated static func icon(_ kind: MDAlertKind) -> String {
        switch kind {
        case .note: "info.circle"
        case .tip: "lightbulb"
        case .important: "exclamationmark.bubble"
        case .warning: "exclamationmark.triangle"
        case .caution: "exclamationmark.octagon"
        }
    }

    var body: some View {
        let colour = Token.fixed(alert.kind.rgb << 8 | 0xff).color
        VStack(alignment: .leading, spacing: metrics.gap * 0.5) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Image(systemName: Self.icon(alert.kind))
                    .font(.system(size: metrics.body, weight: .semibold))
                    .foregroundStyle(colour)
                InlineText.text(alert.title, style: InlineStyle(size: metrics.body, weight: .semibold, color: Palette.text))
                    .foregroundStyle(colour)
                    .fixedSize(horizontal: false, vertical: true)
            }
            if !alert.body.isEmpty {
                BlocksView(blocks: alert.body, metrics: metrics, gap: metrics.gap * 0.5)
            }
        }
        .padding(.vertical, 12)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.markdownAside.color, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        .overlay(alignment: .leading) {
            UnevenRoundedRectangle(topLeadingRadius: 6, bottomLeadingRadius: 6, style: .continuous).fill(colour).frame(width: 4)
        }
        .accessibilityElement(children: .combine)
    }
}

/// The footnotes a document ends with: a rule, then a numbered list at 0.9× with a "↩" per reference.
@MainActor
struct FootnotesView: View {
    let notes: [MDFootnote]
    let metrics: Metrics

    var body: some View {
        let small = Metrics.of(.chat, scale: metrics.body / 14 * 0.9)
        VStack(alignment: .leading, spacing: small.itemGap) {
            Rectangle().fill(Palette.separator.color).frame(height: 1).padding(.bottom, 4)
            ForEach(notes, id: \.number) { note in
                HStack(alignment: .firstTextBaseline, spacing: 6) {
                    Text("\(note.number).")
                        .font(.system(size: small.body).monospacedDigit())
                        .foregroundStyle(Palette.accent.color)
                        .frame(width: small.indent, alignment: .trailing)
                    BlocksView(blocks: Self.withBackrefs(note), metrics: small, gap: small.itemGap)
                }
            }
        }
    }

    /// The note's last paragraph ends with one "↩" per reference; a note ending otherwise gets a paragraph of them.
    nonisolated static func withBackrefs(_ note: MDFootnote) -> [MDBlock] {
        let arrows = [MDInline](repeating: .text(" ↩"), count: note.references)
        var blocks = note.blocks
        if case .paragraph(let inlines)? = blocks.last {
            blocks[blocks.count - 1] = .paragraph(inlines + arrows)
        } else if !arrows.isEmpty {
            blocks.append(.paragraph(arrows))
        }
        return blocks
    }
}
