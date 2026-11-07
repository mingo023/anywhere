import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The agent's `/goal` as a pill, while it's active or paused: its objective and tokens, then Pause or Resume where
/// the agent can hold it, and Clear. A rule sets it apart from the model, as on the desktop.
@MainActor
struct GoalPillView: View {
    let goal: Goal
    let pauses: Bool
    let send: (String) -> Void
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    var body: some View {
        if let words = GoalPill.label(goal.status) {
            let active = goal.status == "active"
            Menu {
                Section("Goal") {
                    Text(goal.objective)
                    if let spent = GoalPill.spent(goal) { Text(spent) }
                }
                if pauses {
                    if active {
                        Button("Pause the goal", systemImage: "stop.fill") { send("pause") }
                    } else {
                        Button("Resume the goal", systemImage: "play.fill") { send("resume") }
                    }
                }
                Button("Clear the goal", systemImage: "xmark") { send("clear") }
            } label: {
                PillLabel(opens: false, fill: active ? Palette.goalBackground : Palette.fill2) {
                    Image(systemName: "sparkles").font(.system(size: 11))
                    Text(words).shimmer(active && !reduceMotion)
                }
                .foregroundStyle((active ? Palette.goalText : Palette.text2).color)
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
            Rectangle().fill(Palette.separator.color).frame(width: 1, height: 16).padding(.horizontal, 4)
        }
    }
}
