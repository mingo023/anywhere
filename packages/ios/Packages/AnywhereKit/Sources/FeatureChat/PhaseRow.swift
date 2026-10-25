import ChatModel
import DesignSystem
import SwiftUI

/// A run of work: its title over its steps, or a lone step on its own. A live phase shimmers, holds open and keeps
/// its last nine steps in view.
@MainActor
struct PhaseRow: View {
    let phase: PhaseContent
    let provider: String
    let actions: RowActions

    var body: some View {
        let _ = BodyCount.hit("PhaseRow")
        if phase.lone, let step = phase.steps.first {
            StepView(step: step, phaseLive: phase.live, provider: provider, actions: actions)
        } else {
            VStack(alignment: .leading, spacing: 2) {
                Button {
                    Haptics.tap()
                    actions.toggle(phase.id, false)
                } label: {
                    HStack(spacing: 6) {
                        Image(systemName: phase.open && !phase.live ? "chevron.down" : phase.glyph.symbol)
                            .font(.system(size: 11, weight: .medium)).frame(width: 14)
                        Text(phase.title).lineLimit(1).shimmer(phase.live)
                    }
                    .font(ChatFont.small)
                    .foregroundStyle(Palette.text3.color)
                    .frame(minHeight: 26)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(phase.live)
                if phase.open {
                    VStack(alignment: .leading, spacing: 2) {
                        ForEach(phase.steps) { step in
                            StepView(step: step, phaseLive: phase.live, provider: provider, actions: actions)
                        }
                    }
                    .padding(.leading, 20)
                }
            }
        }
    }
}

/// One step of a phase: a thought, a tool call or a sub-agent.
@MainActor
struct StepView: View {
    let step: StepContent
    let phaseLive: Bool
    let provider: String
    let actions: RowActions

    var body: some View {
        switch step.block {
        case .thinking(let m):
            ThinkingStep(text: m.text, live: step.live, thought: step.thought, open: step.open) { actions.toggle(step.id, false) }
        case .tool(let call):
            if let agent = call.agent {
                SubAgentRow(call: call, agent: agent, open: step.open, provider: provider, onLink: actions.openLink) {
                    actions.toggle(step.id, call.status == .failed)
                }
            } else {
                ToolRow(call: call, background: call.status == .running && !phaseLive, open: step.open) { actions.toggle(step.id, false) }
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
                    Image(systemName: open ? "chevron.down" : Glyph.bulb.symbol).font(.system(size: 11)).frame(width: 14)
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
