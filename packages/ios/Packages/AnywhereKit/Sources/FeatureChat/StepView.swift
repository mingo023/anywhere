import ChatModel
import DesignSystem
import SwiftUI

/// One step of work on its own line: a thought, a tool call or a sub-agent.
@MainActor
struct StepView: View {
    let step: StepContent
    let actions: RowActions

    var body: some View {
        switch step.block {
        case .thinking(let m):
            ThinkingStep(text: m.text, live: step.live, thought: step.thought, open: step.open) { actions.toggle(step.id, false) }
        case .tool(let call):
            if let agent = call.agent {
                SubAgentRow(call: call, agent: agent) { actions.openAgent(call.id) }
            } else {
                ToolRow(call: call, live: step.live, background: step.background, open: step.open) { actions.toggle(step.id, false) }
            }
        default:
            EmptyView()
        }
    }
}

/// "Thinking" while it runs, "Thought for 3s" after; opens on its faint lines.
@MainActor
struct ThinkingStep: View {
    let text: String
    let live: Bool
    let thought: String?
    let open: Bool
    let toggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 2) {
            Button(action: toggle) {
                HStack(spacing: 6) {
                    if open { LucideIcon(.chevronDown, size: 13).frame(width: 14) } else { Image(systemName: "lightbulb").font(.system(size: 11)).frame(width: 14) }
                    Text(live ? "Thinking" : thought ?? "Thought").shimmer(live)
                }
                .font(ChatFont.small)
                .foregroundStyle(Palette.text3.color)
                .frame(minHeight: 24)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open {
                let lines = thinkingLines(text, live: live)
                VStack(alignment: .leading, spacing: 0) {
                    Text(lines.lines.joined(separator: "\n"))
                    if let more = lines.moreLabel { Text(more).foregroundStyle(Palette.text5.color) }
                }
                .font(ChatFont.caption)
                .lineSpacing(6)
                .foregroundStyle(Palette.text4.color)
                .textSelection(.enabled)
                .padding(.leading, 20)
            }
        }
    }
}
