import ChatModel
import DesignSystem
import SwiftUI
import Wire

/// The agent's `/goal` as a pill, while it's active or paused: its objective and tokens, then Pause or Resume where
/// the agent can hold it, and Clear. A rule sets it apart from the model, as on the desktop. Short of room, only its
/// sparkles show.
@MainActor
struct GoalPillView: View {
    let goal: Goal
    let pauses: Bool
    var fit = PillFit.full
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
                        Button { send("pause") } label: { Label("Pause the goal", lucide: .stop) }
                    } else {
                        Button { send("resume") } label: { Label("Resume the goal", lucide: .play) }
                    }
                }
                Button { send("clear") } label: { Label("Clear the goal", lucide: .close) }
            } label: {
                PillLabel(fill: active ? Palette.goalBackground : Palette.fill3) {
                    LucideIcon(.sparkle, size: 13)
                    if fit == .full { Text(words).shimmer(active && !reduceMotion) }
                }
                .accessibilityLabel(words)
                .foregroundStyle((active ? Palette.goalText : Palette.text2).color)
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
            Rectangle().fill(Palette.separator.color).frame(width: 1, height: 16).padding(.horizontal, 4)
        }
    }
}
