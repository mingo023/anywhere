import ChatModel
import DesignSystem
import SwiftUI

/// A send the transcript doesn't show yet: dimmed while on its way, with a retry once it failed and, under the last
/// steer, when the agent reads it.
@MainActor
struct PendingBubble: View {
    let pending: PendingContent
    let actions: RowActions
    @State private var risen = false
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        VStack(alignment: .trailing, spacing: Space.xs) {
            BubbleBody(text: pending.text, images: pending.images).opacity(0.65)
            if pending.delivery == .failed {
                Button(Bubble.notDelivered) {
                    Haptics.warning()
                    actions.retry(pending.id)
                }
                .buttonStyle(.plain)
                .font(ChatFont.caption)
                .foregroundStyle(Palette.failedText.color)
            }
            if let waiting = pending.waiting {
                HStack(spacing: Space.xs) {
                    Circle().fill(Palette.working.color).frame(width: 5, height: 5)
                    Text(waiting)
                    switch pending.sendNow {
                    case .offer?: Button("· Send now", action: actions.sendNow).buttonStyle(.plain).foregroundStyle(Palette.text.color)
                    case .sending?: Text("Sending…").shimmer()
                    case nil: EmptyView()
                    }
                }
                .font(ChatFont.caption)
                .foregroundStyle(Palette.text3.color)
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
        .offset(y: risen || reduceMotion ? 0 : 12)
        .opacity(risen ? 1 : 0)
        .onAppear {
            withAnimation(reduceMotion ? .easeOut(duration: 0.15) : .easeOut(duration: 0.24)) { risen = true }
        }
    }
}
