import ChatModel
import DesignSystem
import Domain
import MarkdownView
import SwiftUI

/// A spawned agent: its mark, name and what it does now; open, its brief, its steps on a rail and its report.
@MainActor
struct SubAgentRow: View {
    let call: Call
    let agent: SubAgent
    let open: Bool
    let provider: String
    let onLink: (MarkdownLink) -> Void
    let toggle: () -> Void

    private var mark: Mark { Agent.mark(call, agent) }
    private var running: Bool { call.status == .running }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                Haptics.tap()
                toggle()
            } label: {
                head.contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if open { detail.padding(.leading, 20) }
        }
    }

    private var head: some View {
        HStack(spacing: 6) {
            AgentMark(mark: mark)
            Text(Agent.name(agent)).foregroundStyle((mark == .failed ? Palette.failedText : Palette.text2).color).lineLimit(1)
            if running && !open, let latest = Agent.latest(agent) {
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
            Image(systemName: open ? "chevron.down" : "chevron.right").font(.system(size: 9, weight: .semibold)).foregroundStyle(Palette.text4.color)
        }
        .font(ChatFont.small)
        .frame(minHeight: 26)
    }

    @ViewBuilder
    private var detail: some View {
        if let brief = Agent.brief(agent) {
            ClippedText(lines: Agent.briefLines, estimate: Agent.lineCount(brief)) {
                Text(brief).font(ChatFont.small).lineSpacing(7).foregroundStyle(Palette.text3.color)
            }
        }
        ForEach(Array(Agent.steps(agent).enumerated()), id: \.offset) { _, block in
            switch block {
            // Nested sub-agents show as tool rows; the agent map holds the tree.
            case .tool(let c):
                ToolRow(call: c, background: false, open: false) {}
            case .text(let m):
                Text(m.text).font(ChatFont.small).foregroundStyle(Palette.text3.color).lineLimit(3)
            default:
                EmptyView()
            }
        }
        if let report = Agent.report(call, agent) {
            ClippedText(lines: Agent.reportLines, estimate: Agent.lineCount(report)) {
                if mark == .failed {
                    Text(report).font(ChatFont.mono).foregroundStyle(Palette.failedText.color)
                } else {
                    MarkdownView(report, style: .compact, onLink: onLink)
                }
            }
        }
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
