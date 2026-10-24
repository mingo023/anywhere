import ChatModel
import DesignSystem
import SwiftUI

/// How full the context window is, as a 12 pt ring: amber from 75%, red from 90%; a tap tells the tokens.
@MainActor
struct ContextRing: View {
    let used: Int
    let window: Int
    @State private var open = false

    var body: some View {
        let percent = ContextMeter.percent(used, window)
        Button {
            open = true
        } label: {
            ZStack {
                Circle().stroke(Palette.fill4.color, lineWidth: 2)
                Circle().trim(from: 0, to: Double(percent) / 100).rotation(.degrees(-90))
                    .stroke(tint.color, style: StrokeStyle(lineWidth: 2, lineCap: .round))
            }
            .frame(width: 12, height: 12)
            .frame(width: 28, height: 28)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Context \(percent)% full")
        .popover(isPresented: $open) {
            let (usedLine, remaining) = ContextMeter.lines(used, window)
            VStack(alignment: .leading, spacing: 4) {
                Text("Context window").font(ChatFont.small.weight(.semibold))
                Text(usedLine).font(ChatFont.caption).monospacedDigit().foregroundStyle(Palette.text2.color)
                Text(remaining).font(ChatFont.caption).monospacedDigit().foregroundStyle(Palette.text3.color)
            }
            .padding(Space.m)
            .presentationCompactAdaptation(.popover)
        }
    }

    private var tint: Token {
        switch ContextMeter.level(used, window) {
        case .low: Palette.text3
        case .warn: Palette.waiting
        case .danger: Palette.failed
        }
    }
}
