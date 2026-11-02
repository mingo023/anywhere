import DesignSystem
import SwiftUI

/// A file the agent shared: its name and caption; a tap opens it in the file preview when the phone can show it.
@MainActor
struct SharedCard: View {
    let shared: SharedContent
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            HStack(spacing: 8) {
                Image(systemName: "doc").font(.system(size: 11)).foregroundStyle(Palette.text2.color)
                    .frame(width: 22, height: 22).background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: 6))
                Text(shared.name).foregroundStyle(Palette.text2.color).lineLimit(1).layoutPriority(1)
                Text(shared.detail).foregroundStyle((shared.failed ? Palette.text3 : Palette.text).color).lineLimit(1)
                if shared.opens != nil {
                    Image(systemName: "chevron.right").font(.system(size: 10, weight: .semibold)).foregroundStyle(Palette.text3.color)
                }
            }
            .font(ChatFont.small)
            .padding(.leading, 6)
            .padding(.trailing, 12)
            .frame(height: 34)
            .background(Palette.fill1.color, in: Capsule())
            .overlay(Capsule().strokeBorder(Palette.hairline.color))
        }
        .buttonStyle(.plain)
        .disabled(shared.opens == nil)
    }
}
