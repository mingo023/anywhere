import ChatModel
import DesignSystem
import SwiftUI

/// The 34 pt send button: Send, Steer, Queue, Stop or Stopping, dimmed where the agent can't take the action. Armed,
/// it turns while images upload and a tap disarms it. With `queue`, a long press offers queueing for after the turn.
@MainActor
struct SendButton: View {
    let action: ChatModel.Action
    let empty: Bool
    let allowed: Bool
    /// The Mac is online.
    let enabled: Bool
    var armed = false
    var queue: (() -> Void)?
    let press: () -> Void

    var body: some View {
        Group {
            if let queue {
                Menu {
                    Button("Send into this turn", systemImage: "arrow.up") { tapped() }
                    Button("Queue for after this turn", systemImage: "list.bullet") {
                        Haptics.tap()
                        queue()
                    }
                } label: {
                    label.hitTarget()
                } primaryAction: {
                    tapped()
                }
                .menuStyle(.button)
                .menuIndicator(.hidden)
                .fixedSize()
            } else {
                Button(action: tapped) { label.hitTarget() }
            }
        }
        .buttonStyle(.plain)
        .disabled(!enabled || (!armed && (action == .stopping || (action == .send && empty))))
        .opacity(allowed && enabled ? 1 : 0.35)
        .accessibilityLabel(armed ? "Cancel send" : Self.label(action))
    }

    private func tapped() {
        Haptics.tap()
        press()
    }

    @ViewBuilder
    private var label: some View {
        switch action {
        case _ where armed:
            ZStack {
                Circle().fill(Palette.text.color)
                RingSpinner(size: 26, tint: Palette.onText)
                Image(systemName: "arrow.up").font(.system(size: 13, weight: .bold)).foregroundStyle(Palette.onText.color)
            }
            .frame(width: 34, height: 34)
        case .send, .steer:
            Image(systemName: "arrow.up")
                .font(.system(size: 15, weight: .bold))
                .foregroundStyle((empty ? Palette.text4 : Palette.onText).color)
                .frame(width: 34, height: 34)
                .background((empty ? Palette.fill3 : Palette.text).color, in: Circle())
        case .queue:
            Text("Queue").font(ChatFont.small.weight(.semibold)).foregroundStyle(Palette.onText.color)
                .padding(.horizontal, 12).frame(height: 34).background(Palette.text.color, in: Capsule())
        case .stop, .stopping:
            ZStack {
                Circle().fill(Palette.text.color)
                if action == .stop {
                    RingSpinner(size: 26, tint: Palette.onText)
                }
                RoundedRectangle(cornerRadius: 2).fill(Palette.onText.color).frame(width: 10, height: 10)
            }
            .frame(width: 34, height: 34)
            .opacity(action == .stopping ? 0.5 : 1)
        }
    }

    nonisolated static func label(_ action: ChatModel.Action) -> String {
        switch action {
        case .send: "Send"
        case .steer: "Steer"
        case .queue: "Queue"
        case .stop: "Stop"
        case .stopping: "Stopping"
        }
    }
}

/// The ghost Stop beside Steer, while the agent can be stopped.
@MainActor
struct GhostStop: View {
    let enabled: Bool
    let stop: () -> Void

    var body: some View {
        Button {
            Haptics.warning()
            stop()
        } label: {
            RoundedRectangle(cornerRadius: 2).fill(Palette.text2.color).frame(width: 10, height: 10)
                .frame(width: 34, height: 34)
                .overlay(Circle().strokeBorder(Palette.separatorStrong.color))
                .hitTarget()
        }
        .buttonStyle(.plain)
        .disabled(!enabled)
        .opacity(enabled ? 1 : 0.35)
        .accessibilityLabel("Stop")
    }
}
