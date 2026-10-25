import DesignSystem
import SwiftUI

/// A `/btw` side question and its answer, over the composer.
@MainActor
struct BtwCard: View {
    let question: String
    let answer: String?
    let error: String?
    let close: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 8) {
                Text("/btw").font(ChatFont.mono.weight(.semibold)).foregroundStyle(Palette.command.color)
                Text(question).font(ChatFont.small).foregroundStyle(Palette.text.color).lineLimit(1)
                Spacer(minLength: 0)
                Button(action: close) { Image(systemName: "xmark").font(.system(size: 12)).frame(width: 28, height: 28) }
                    .buttonStyle(.plain).foregroundStyle(Palette.text3.color).accessibilityLabel("Close")
            }
            ScrollView {
                Group {
                    if let answer {
                        Text(answer).foregroundStyle(Palette.textBody.color).textSelection(.enabled)
                    } else if let error {
                        Text(error).foregroundStyle(Palette.text3.color)
                    } else {
                        Text("Thinking…").foregroundStyle(Palette.text4.color).shimmer()
                    }
                }
                .font(ChatFont.small)
                .frame(maxWidth: .infinity, alignment: .leading)
            }
            .frame(maxHeight: 220)
            .fixedSize(horizontal: false, vertical: true)
        }
        .trayCard()
    }
}
