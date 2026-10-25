import DesignSystem
import SwiftUI

/// Where the agent asked a question: "Question" and the first one's header, or that it waits for an answer.
@MainActor
struct QuestionMarker: View {
    let value: String

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "text.bubble").font(.system(size: 11)).foregroundStyle(Palette.text2.color)
                .frame(width: 22, height: 22).background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 6))
            Text("Question").foregroundStyle(Palette.text2.color)
            Text(value).foregroundStyle(Palette.text3.color).lineLimit(1)
        }
        .font(ChatFont.small)
        .padding(.leading, 6)
        .padding(.trailing, 12)
        .frame(height: 34)
        .background(Palette.fill1.color, in: Capsule())
        .overlay(Capsule().strokeBorder(Palette.hairline.color))
    }
}
