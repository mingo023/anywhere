import SwiftUI

struct Pill: ButtonStyle {
    let fill: Color
    let hover: Color
    let ink: Color
    var hoverInk: Color?
    var padding: CGFloat = 12

    func makeBody(configuration: Configuration) -> some View {
        Pressable(configuration: configuration, style: self)
    }

    private struct Pressable: View {
        let configuration: Configuration
        let style: Pill
        @State private var hovered = false

        var body: some View {
            configuration.label
                .font(.system(size: 13, weight: .medium))
                .foregroundColor(hovered ? style.hoverInk ?? style.ink : style.ink)
                .padding(.horizontal, style.padding)
                .frame(height: 28)
                .background(RoundedRectangle(cornerRadius: 7).fill(hovered ? style.hover : style.fill))
                .contentShape(Rectangle())
                .opacity(configuration.isPressed ? 0.85 : 1)
                .onHover { hovered = $0 }
        }
    }
}
