import ChatModel
import DesignSystem
import Domain
import SwiftUI

/// The user's prompt: right-aligned, its commands tinted and mentions shown by name, clipped past five lines. A long
/// press copies it and says when it was sent.
@MainActor
struct PromptBubble: View {
    let text: String
    let images: [String]
    let at: Int?
    var dimmed = false

    var body: some View {
        let _ = BodyCount.hit("PromptBubble")
        BubbleBody(text: text, images: images)
            .opacity(dimmed ? 0.65 : 1)
            .contextMenu {
                Button { Clipboard.copy(text) } label: { Label("Copy", lucide: .copy) }
                if let at { Text("Sent \(TimeText.clock(ms: at))") }
            }
            .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

/// The bubble itself, shared by prompts in the transcript and sends on their way.
@MainActor
struct BubbleBody: View {
    let text: String
    let images: [String]
    @State private var expanded = false
    @State private var fullHeight: CGFloat = 0

    private var collapses: Bool { fullHeight > CGFloat(Bubble.collapsedLines) * Bubble.lineHeight + 1 }

    var body: some View {
        FractionWidth(fraction: Bubble.width) {
            VStack(alignment: .leading, spacing: Space.s) {
                if !images.isEmpty { ImageTiles(paths: images) }
                if !text.isEmpty {
                    BubbleText(text: text)
                        .lineLimit(expanded ? nil : Bubble.collapsedLines)
                        .background(alignment: .topLeading) {
                            BubbleText(text: text).fixedSize(horizontal: false, vertical: true).hidden()
                                .onHeight { fullHeight = $0 }
                        }
                    if collapses {
                        Button {
                            withAnimation(.timingCurve(0.22, 1, 0.36, 1, duration: Bubble.tween(fullHeight - CGFloat(Bubble.collapsedLines) * Bubble.lineHeight))) {
                                expanded.toggle()
                            }
                        } label: {
                            HStack(spacing: Space.xs) {
                                Text(expanded ? "Show less" : "Show more")
                                LucideIcon(expanded ? .chevronUp : .chevronDown, size: 12)
                            }
                            .font(ChatFont.caption.weight(.medium))
                            .foregroundStyle(Palette.text3.color)
                        }
                        .buttonStyle(.plain)
                    }
                }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .background(Palette.bubble.color, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
        }
    }
}

/// A prompt's text with `/commands` tinted and `@path` mentions as a file icon and the name.
@MainActor
struct BubbleText: View {
    let text: String

    var body: some View {
        Self.styled(text)
            .font(ChatFont.bubble)
            .lineSpacing(Bubble.lineHeight - Bubble.textSize * 1.19)
            .foregroundStyle(Palette.text.color)
            .textSelection(.enabled)
    }

    static func styled(_ text: String) -> Text {
        Bubble.segments(text).reduce(Text(verbatim: "")) { acc, s in
            switch s.kind {
            case .plain: Text("\(acc)\(Text(verbatim: s.text))")
            case .command: Text("\(acc)\(Text(verbatim: s.text).foregroundStyle(Palette.command.color))")
            case .mention: Text("\(acc)\(Text("\(Image.lucide(.file, size: 13)) \(s.text)").foregroundStyle(Palette.mention.color))")
            }
        }
    }
}

/// Attached images as 56 pt tiles; the phone sees only their paths on the Mac, so a tile names the file.
@MainActor
struct ImageTiles: View {
    let paths: [String]

    var body: some View {
        HStack(spacing: Space.xs) {
            ForEach(paths, id: \.self) { path in
                VStack(spacing: 2) {
                    LucideIcon(.imagePlus, size: 18)
                    Text(FileChip.name(path)).font(.system(size: 8)).lineLimit(1).truncationMode(.middle)
                }
                .foregroundStyle(Palette.text3.color)
                .padding(4)
                .frame(width: 56, height: 56)
                .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 8))
                .accessibilityLabel(path)
            }
        }
    }
}

/// Lays its content out at most `fraction` of the width offered, at the trailing edge.
struct FractionWidth<Content: View>: View {
    let fraction: CGFloat
    @ViewBuilder let content: Content

    var body: some View {
        FractionLayout(fraction: fraction) { content }
    }
}

private struct FractionLayout: Layout {
    let fraction: CGFloat

    func sizeThatFits(proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) -> CGSize {
        guard let child = subviews.first else { return .zero }
        let size = child.sizeThatFits(ProposedViewSize(width: proposal.width.map { $0 * fraction }, height: nil))
        return CGSize(width: proposal.width ?? size.width, height: size.height)
    }

    func placeSubviews(in bounds: CGRect, proposal: ProposedViewSize, subviews: Subviews, cache: inout ()) {
        guard let child = subviews.first else { return }
        let size = child.sizeThatFits(ProposedViewSize(width: bounds.width * fraction, height: nil))
        child.place(at: CGPoint(x: bounds.maxX - size.width, y: bounds.minY), proposal: ProposedViewSize(size))
    }
}
