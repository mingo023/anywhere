import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The agent's `/goal`: its objective, how it stands and the tokens spent, with Pause (Codex) and Clear.
@MainActor
struct GoalStripView: View {
    let goal: Goal
    let pauses: Bool
    let send: (String) -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "sparkles").font(.system(size: 12)).foregroundStyle(Palette.text2.color)
            VStack(alignment: .leading, spacing: 1) {
                Text(goal.objective).font(ChatFont.small).foregroundStyle(Palette.text.color).lineLimit(1)
                Text([GoalStrip.statusLabel(goal.status), GoalStrip.spent(goal)].compactMap { $0?.isEmpty == false ? $0 : nil }.joined(separator: " · "))
                    .font(ChatFont.caption).monospacedDigit().foregroundStyle(Palette.text3.color)
            }
            Spacer(minLength: 0)
            if pauses && goal.status == "active" {
                Button { send("pause") } label: { Image(systemName: "stop.fill").font(.system(size: 11)).frame(width: 28, height: 28) }
                    .accessibilityLabel("Pause the goal")
            }
            Button { send("clear") } label: { Image(systemName: "xmark").font(.system(size: 12)).frame(width: 28, height: 28) }
                .accessibilityLabel("Clear the goal")
        }
        .buttonStyle(.plain)
        .foregroundStyle(Palette.text3.color)
        .trayCard()
    }
}
