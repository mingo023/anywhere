import ChatModel
import DesignSystem
import Domain
import SwiftUI

/// Sub-agents and commands left running past their turn: folded to the newest step, open to a row each that shows
/// where it ran.
@MainActor
struct BackgroundTray: View {
    let tasks: [Call]
    let stops: Bool
    let stop: () -> Void
    let show: (String) -> Void
    @State private var folded = true

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Button {
                withAnimation(.easeOut(duration: 0.2)) { folded.toggle() }
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: Glyph.bot.symbol).font(.system(size: 12)).foregroundStyle(Palette.text2.color)
                    Text("\(tasks.count) in background").fontWeight(.semibold).foregroundStyle(Palette.text.color).fixedSize()
                    Text(folded ? Background.newest(tasks) ?? "" : "").font(ChatFont.monoSmall).foregroundStyle(Palette.text3.color).lineLimit(1)
                    Spacer(minLength: 0)
                    if stops {
                        Button {
                            Haptics.warning()
                            stop()
                        } label: {
                            HStack(spacing: 6) {
                                RoundedRectangle(cornerRadius: 2).fill(Palette.text2.color).frame(width: 8, height: 8)
                                Text("Stop")
                            }
                            .frame(minHeight: 28)
                        }
                        .buttonStyle(.plain)
                        .foregroundStyle(Palette.text2.color)
                        .accessibilityHint("Stops the background agents")
                    }
                    Image(systemName: folded ? "chevron.right" : "chevron.down").font(.system(size: 11)).foregroundStyle(Palette.text4.color)
                }
                .font(ChatFont.caption)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            if !folded {
                ForEach(tasks, id: \.id) { call in
                    Button {
                        show(call.id)
                    } label: {
                        HStack(spacing: 8) {
                            if let agent = call.agent { AgentMark(mark: Agent.mark(call, agent)) } else { RingSpinner(size: 12) }
                            Text(Background.name(call)).font(ChatFont.small).foregroundStyle(Palette.text2.color).lineLimit(1)
                            if let doing = Background.doing(call) {
                                Text(doing).font(ChatFont.monoSmall).foregroundStyle(Palette.text4.color).lineLimit(1)
                            }
                            Spacer(minLength: 0)
                            TimelineView(.periodic(from: .now, by: 1)) { context in
                                Text(TimeText.elapsed(seconds: AgentMap.ranMs(call, Int(context.date.timeIntervalSince1970 * 1000)) / 1000)).monospacedDigit()
                            }
                            .font(ChatFont.caption)
                            .foregroundStyle(Palette.text4.color)
                            HStack(spacing: 3) {
                                Text("Show")
                                Image(systemName: "arrow.up").font(.system(size: 10))
                            }
                            .font(ChatFont.caption)
                            .foregroundStyle(Palette.text4.color)
                        }
                        .frame(minHeight: 30)
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
        }
        .trayCard()
    }
}
