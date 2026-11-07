import ChatModel
import DesignSystem
import SwiftUI

/// Who spawned whom, as a sheet: a column per generation with dashes flowing to the agents at work. Picking a sub-agent
/// opens its own screen in the chat's stack.
@MainActor
struct AgentMapSheet: View {
    let transcript: Transcript
    let provider: String
    let openAgent: (_ callId: String) -> Void
    @State private var nav = AgentMap.Nav()
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let map = spawned(transcript)
        NavigationStack {
            graph(map)
                .navigationTitle("Agents")
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
        }
        .onChange(of: nav.detail) { _, id in
            guard let id else { return }
            dismiss()
            openAgent(id)
        }
        .onKeyPress(.escape) {
            dismiss()
            return .handled
        }
        .onKeyPress(keys: [.upArrow, .downArrow, .return, .rightArrow]) { press in
            let key: AgentMap.Nav.Key =
                switch press.key {
                case .upArrow: .up
                case .downArrow: .down
                case .rightArrow: .right
                default: .enter
                }
            return nav.key(key, map.map(\.call.id), Date.timeIntervalSinceReferenceDate) ? .handled : .ignored
        }
    }

    private func graph(_ map: [Spawned]) -> some View {
        let (at, size) = Graph.layout(map.map(\.parent))
        let running = map.contains { $0.call.status == .running }
        return ScrollView([.horizontal, .vertical]) {
            VStack(alignment: .leading, spacing: Space.s) {
                Text(AgentMap.summary(map)).font(ChatFont.small).foregroundStyle(Palette.text3.color).padding(.horizontal, Graph.pad)
                ZStack(alignment: .topLeading) {
                    TimelineView(.animation(minimumInterval: 1.0 / 20, paused: !running)) { context in
                        Canvas { g, _ in
                            let phase = context.date.timeIntervalSinceReferenceDate * 16
                            for (i, s) in map.enumerated() {
                                let p = s.parent.map { $0 + 1 } ?? 0
                                let from = (x: at[p].x + Graph.width, y: at[p].y + Graph.height / 2)
                                let to = (x: at[i + 1].x, y: at[i + 1].y + Graph.height / 2)
                                let points = Graph.elbow(from, to)
                                let pieces = s.call.status == .running ? Graph.dashes(points, phase: phase) : [points]
                                for piece in pieces {
                                    var path = Path()
                                    path.addLines(piece.map { CGPoint(x: $0.x, y: $0.y) })
                                    g.stroke(path, with: .color((s.call.status == .running ? Palette.success : Palette.rail).color), lineWidth: 1.5)
                                }
                            }
                        }
                    }
                    .frame(width: size.x, height: size.y)
                    node(selected: nav.selected == 0) {
                        BrandMark(provider)
                        VStack(alignment: .leading, spacing: 2) {
                            Text(who(provider)).font(ChatFont.small.weight(.medium))
                            Text(AgentMap.mainLine(transcript, map)).font(ChatFont.caption).foregroundStyle(Palette.text3.color).lineLimit(1)
                        }
                    }
                    .offset(x: at[0].x, y: at[0].y)
                    ForEach(Array(map.enumerated()), id: \.offset) { i, s in
                        Button {
                            nav.show(i + 1, s.call.id)
                        } label: {
                            node(selected: nav.selected == i + 1) {
                                AgentMark(mark: Agent.mark(s.call, s.agent))
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(Agent.name(s.agent)).font(ChatFont.small.weight(.medium)).lineLimit(1)
                                    TimelineView(.periodic(from: .now, by: 1)) { context in
                                        Text(AgentMap.meta(s.call, tokens: s.agent.tokens, now: Int(context.date.timeIntervalSince1970 * 1000)))
                                            .monospacedDigit()
                                    }
                                    .font(ChatFont.caption)
                                    .foregroundStyle((s.call.status == .failed ? Palette.failedText : Palette.text3).color)
                                }
                            }
                        }
                        .buttonStyle(.plain)
                        .offset(x: at[i + 1].x, y: at[i + 1].y)
                    }
                }
                .frame(width: size.x, height: size.y, alignment: .topLeading)
            }
            .padding(.vertical, Space.m)
        }
        .screenBackground()
    }

    private func node<Content: View>(selected: Bool, @ViewBuilder _ content: () -> Content) -> some View {
        HStack(spacing: 8) { content() }
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, 12)
            .frame(width: Graph.width, height: Graph.height, alignment: .leading)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 12, style: .continuous).strokeBorder((selected ? Palette.separatorStrong : Palette.hairline).color))
    }
}
