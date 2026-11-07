import ChatModel
import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI

/// One sub-agent as it works: who spawned it, how long it ran, what it's on now, its prompt, each step as it lands and
/// what it reported.
@MainActor
struct SubAgentScreen: View {
    let store: ChatStore
    let callId: String
    let openAgent: (_ callId: String) -> Void
    let openLink: (MarkdownLink) -> Void
    /// Off for gallery scenes, which have no Mac to tell.
    var live = true
    @State private var open: Set<String> = []

    var body: some View {
        Group {
            if let (call, agent, by) = subAgent(store.session.transcript, callId) {
                page(call, agent, by).barTitle(Agent.name(agent), subtitle: AgentMap.facts(Agent.mark(call, agent), model: agent.model))
            } else {
                Placeholder("This agent isn't in the chat any more", systemImage: "person.2.slash").navigationTitle("Agent")
            }
        }
        .screenBackground()
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .onAppear { if live { store.appear() } }
        .onDisappear { if live { store.disappear() } }
    }

    private func page(_ call: Call, _ agent: SubAgent, _ by: SubAgent?) -> some View {
        let mark = Agent.mark(call, agent)
        let running = call.status == .running
        let steps = Agent.steps(agent)
        return ScrollView {
            LazyVStack(alignment: .leading, spacing: Space.s) {
                TimelineView(.periodic(from: .now, by: 1)) { context in
                    let now = Int(context.date.timeIntervalSince1970 * 1000)
                    VStack(alignment: .leading, spacing: Space.m) {
                        HStack(spacing: 8) {
                            AgentMark(mark: mark)
                            Text(AgentMap.origin(by: by, call, now: now)).font(ChatFont.caption).foregroundStyle(Palette.text3.color)
                        }
                        HStack(spacing: Space.s) {
                            stat(TimeText.elapsed(seconds: AgentMap.ranMs(call, now) / 1000), running ? "so far" : "ran for")
                            stat(agent.tokens == 0 ? "—" : AgentMap.short(agent.tokens), "tokens")
                            stat("\(AgentMap.calls(agent))", "tool calls")
                        }
                    }
                }
                if running, let latest = Agent.latest(agent) {
                    let (tool, arg) = AgentMap.step(latest)
                    HStack(spacing: 8) {
                        AgentMark(mark: .running)
                        Text("Now")
                        Text(tool).fontWeight(.semibold)
                        Text(arg).font(ChatFont.mono).foregroundStyle(Palette.text2.color).lineLimit(1).truncationMode(.middle)
                    }
                    .font(ChatFont.small)
                    .foregroundStyle(Palette.accent.color)
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Palette.accentBackground.color, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
                if let brief = Agent.brief(agent) {
                    section("Prompt")
                    ClippedText(lines: Agent.briefLines, estimate: Agent.lineCount(brief)) {
                        Text(brief).font(ChatFont.small).lineSpacing(5).foregroundStyle(Palette.textBody.color)
                            .frame(maxWidth: .infinity, alignment: .leading)
                    }
                }
                section("Steps", count: AgentMap.calls(agent))
                ForEach(Array(steps.enumerated()), id: \.offset) { _, block in
                    step(block)
                }
                if !running, let report = Agent.report(call, agent) {
                    section(mark == .failed ? "Error" : "Report")
                    Group {
                        if mark == .failed {
                            Text(report).font(ChatFont.mono).foregroundStyle(Palette.failedText.color).textSelection(.enabled)
                        } else {
                            MarkdownView(report, style: .compact, onLink: openLink)
                        }
                    }
                    .padding(12)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background((mark == .failed ? Palette.failedBackground : Palette.surface).color, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                }
            }
            .padding(Space.l)
        }
    }

    @ViewBuilder
    private func step(_ block: Block) -> some View {
        switch block {
        case .tool(let c):
            if let a = c.agent {
                SubAgentRow(call: c, agent: a) { openAgent(c.id) }
            } else {
                ToolRow(call: c, background: false, open: open.contains(c.id)) {
                    if open.remove(c.id) == nil { open.insert(c.id) }
                }
            }
        case .text(let m):
            Text(m.text).font(ChatFont.small).foregroundStyle(Palette.text3.color).lineLimit(3)
        default:
            EmptyView()
        }
    }

    private func section(_ title: String, count: Int? = nil) -> some View {
        HStack(spacing: 6) {
            Text(title).foregroundStyle(Palette.text2.color)
            if let count { Text("\(count)").fontWeight(.medium).foregroundStyle(Palette.text4.color) }
        }
        .font(ChatFont.caption.weight(.semibold))
        .padding(.top, Space.s)
    }

    private func stat(_ value: String, _ label: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(value).font(.system(size: 15, weight: .semibold)).monospacedDigit().foregroundStyle(Palette.text.color)
            Text(label).font(.system(size: 11)).foregroundStyle(Palette.text3.color)
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Palette.fill2.color, in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}
