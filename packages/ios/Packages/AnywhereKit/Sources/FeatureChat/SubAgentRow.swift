import ChatModel
import DesignSystem
import Domain
import SwiftUI

/// A spawned agent: its mark, name and what it does now; a tap opens its own screen.
@MainActor
struct SubAgentRow: View {
    let call: Call
    let agent: SubAgent
    let open: () -> Void

    private var mark: Mark { Agent.mark(call, agent) }
    private var running: Bool { call.status == .running }

    var body: some View {
        Button {
            Haptics.tap()
            open()
        } label: {
            head.contentShape(Rectangle())
        }
        .buttonStyle(.plain)
    }

    private var head: some View {
        HStack(spacing: 6) {
            AgentMark(mark: mark)
            Text(Agent.name(agent)).foregroundStyle((mark == .failed ? Palette.failedText : Palette.text2).color).lineLimit(1)
            if running, let latest = Agent.latest(agent) {
                Text(Agent.step(latest)).font(ChatFont.mono).foregroundStyle(Palette.text4.color).lineLimit(1).truncationMode(.tail)
            }
            Spacer(minLength: Space.s)
            Group {
                if !agent.model.isEmpty { Text(modelName(agent.model)) }
                Text(Agent.meta(call, agent))
                if running {
                    TimelineView(.periodic(from: .now, by: 1)) { context in
                        Text(TimeText.elapsed(seconds: max(Int(context.date.timeIntervalSince1970 * 1000) - call.at, 0) / 1000)).monospacedDigit()
                    }
                }
            }
            .font(ChatFont.caption)
            .foregroundStyle(Palette.text4.color)
            Image(systemName: "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(Palette.text4.color)
        }
        .font(ChatFont.small)
        .frame(minHeight: 26)
    }
}

/// Where a sub-agent stands: a dotted ring while starting, a turning arc while running, a check, or a red cross.
@MainActor
struct AgentMark: View {
    let mark: Mark

    var body: some View {
        Group {
            switch mark {
            case .starting: Circle().strokeBorder(Palette.text5.color, style: StrokeStyle(lineWidth: 1.5, dash: [2, 2]))
            case .running: DotSpinner(size: 12)
            case .done: Image(systemName: "checkmark.circle.fill").foregroundStyle(Palette.success.color)
            case .failed: Image(systemName: "xmark.circle.fill").foregroundStyle(Palette.failed.color)
            }
        }
        .font(.system(size: 12))
        .frame(width: 14, height: 14)
    }
}

/// Text clipped to `lines` lines with "Show more" when it runs longer, as estimated without laying it out.
@MainActor
struct ClippedText<Content: View>: View {
    let lines: Int
    let estimate: Int
    @ViewBuilder let content: Content
    @State private var expanded = false

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            content
                .frame(maxHeight: expanded || estimate <= lines ? nil : CGFloat(lines) * 21, alignment: .top)
                .clipped()
            if estimate > lines {
                Button(expanded ? "Show less" : "Show more") { withAnimation(.easeOut(duration: 0.22)) { expanded.toggle() } }
                    .buttonStyle(.plain)
                    .font(ChatFont.caption.weight(.medium))
                    .foregroundStyle(Palette.text3.color)
            }
        }
    }
}
