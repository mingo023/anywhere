import DesignSystem
import SwiftUI

/// How a turn ended: stopped, an error card, or the reply's footer once nothing works on in it.
@MainActor
struct EndRow: View {
    let end: EndContent
    let actions: RowActions
    @State private var selecting = false

    var body: some View {
        switch end {
        case .interrupted(let text):
            Text(text).font(ChatFont.small).foregroundStyle(Palette.text3.color)
        case .failed(let error):
            VStack(alignment: .leading, spacing: 6) {
                HStack(spacing: 8) {
                    Image(systemName: "exclamationmark.triangle.fill").font(.system(size: 11)).foregroundStyle(Palette.failedText.color)
                        .frame(width: 22, height: 22).background(Palette.failedBackground.color, in: RoundedRectangle(cornerRadius: 6))
                    Text("Error").font(ChatFont.small.weight(.semibold)).foregroundStyle(Palette.text.color)
                    Spacer()
                    CopyButton(text: error)
                }
                Text(error).font(ChatFont.small).foregroundStyle(Palette.textBody.color).textSelection(.enabled)
            }
            .padding(Space.m)
            .background(Palette.failedBackground.color.opacity(0.5), in: RoundedRectangle(cornerRadius: 12))
            .overlay(RoundedRectangle(cornerRadius: 12).strokeBorder(Palette.hairline.color))
        case .done(let footer?):
            HStack(spacing: Space.m) {
                CopyButton(text: footer.answer, label: "Copy answer")
                #if os(iOS)
                    Button {
                        selecting = true
                    } label: {
                        Image(systemName: "text.cursor").frame(minWidth: 28, minHeight: 28).contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Select text")
                    .font(ChatFont.caption)
                    .foregroundStyle(Palette.text3.color)
                    .sheet(isPresented: $selecting) { SelectTextSheet(footer.answer) }
                #endif
                if let turn = footer.retry {
                    Button {
                        Haptics.tap()
                        actions.retryTurn(turn)
                    } label: {
                        Label("Retry this turn", systemImage: "arrow.clockwise").frame(minHeight: 28)
                    }
                    .buttonStyle(.plain)
                    .font(ChatFont.caption)
                    .foregroundStyle(Palette.text3.color)
                }
                Spacer(minLength: 0)
                Text(footer.stats).font(ChatFont.caption).monospacedDigit().foregroundStyle(Palette.text4.color)
            }
        case .done(nil):
            Color.clear.frame(height: 0)
        }
    }
}
