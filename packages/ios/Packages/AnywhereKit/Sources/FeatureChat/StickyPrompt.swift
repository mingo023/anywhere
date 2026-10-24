import ChatModel
import DesignSystem
import Domain
import SwiftUI

/// The exchange's prompt floated over the chat's top once it scrolls past: one line, opening on the whole prompt.
/// The next prompt pushes it out rather than sliding under it.
@MainActor
struct StickyPrompt: View {
    let text: String
    let images: [String]
    let at: Int
    /// How far the next prompt pushed it up, as `Sticky.pushed` has it; zero or less.
    let shift: Double
    let jump: () -> Void
    @State private var open = false

    private var line: String {
        let one = Sticky.oneLine(text)
        return one.isEmpty ? Sticky.imageCount(images.count) : one
    }

    private var long: Bool { text.contains("\n") || line.count > 48 }

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Button(action: jump) {
                Image(systemName: "arrow.up").font(.system(size: 11, weight: .semibold)).foregroundStyle(Palette.text3.color)
                    .frame(width: Sticky.knob, height: Sticky.knob)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Go to the prompt")
            ForEach(images.prefix(Sticky.thumbs), id: \.self) { _ in
                Image(systemName: "photo").font(.system(size: 10)).frame(width: 20, height: 20)
                    .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 4)).padding(.top, 1)
            }
            if images.count > Sticky.thumbs {
                Text("+\(images.count - Sticky.thumbs)").font(ChatFont.caption).foregroundStyle(Palette.text3.color).frame(height: Sticky.knob)
            }
            Group {
                if open {
                    ScrollView {
                        Text(text).frame(maxWidth: .infinity, alignment: .leading).textSelection(.enabled)
                    }
                    .frame(maxHeight: Sticky.openMaxHeight)
                    .fixedSize(horizontal: false, vertical: true)
                } else {
                    Text(line).lineLimit(1).truncationMode(.tail)
                }
            }
            .font(ChatFont.small)
            .foregroundStyle(Palette.text.color)
            .padding(.top, 1)
            .frame(minHeight: Sticky.knob, alignment: .top)
            .contentShape(Rectangle())
            .onTapGesture(perform: jump)
            HStack(spacing: 4) {
                Text("·").foregroundStyle(Palette.text5.color)
                Text(TimeText.clock(ms: at)).monospacedDigit().foregroundStyle(Palette.text3.color)
            }
            .font(ChatFont.caption)
            .frame(height: Sticky.knob)
            if long {
                Button {
                    withAnimation(.easeOut(duration: 0.2)) { open.toggle() }
                } label: {
                    Image(systemName: open ? "chevron.up" : "chevron.down").font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(Palette.text3.color).frame(width: Sticky.knob, height: Sticky.knob)
                }
                .buttonStyle(.plain)
                .accessibilityLabel(open ? "Fold the prompt" : "Show the whole prompt")
            }
        }
        .padding(.horizontal, 6)
        .padding(.top, open ? Sticky.openTop : (Sticky.height - Sticky.knob) / 2)
        .padding(.bottom, open ? Sticky.openBottom : (Sticky.height - Sticky.knob) / 2)
        .background(Palette.popover.color, in: RoundedRectangle(cornerRadius: Sticky.height / 2, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Sticky.height / 2, style: .continuous).strokeBorder(Palette.hairline.color))
        .shadow(color: .black.opacity(0.12 * Sticky.lift(shift)), radius: 10, y: 3)
        .offset(y: shift)
        .padding(.horizontal, Space.l)
        .padding(.top, Sticky.marginTop)
        .transition(.opacity.combined(with: .offset(y: -6)))
    }
}
