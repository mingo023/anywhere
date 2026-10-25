import ChatModel
import DesignSystem
import SwiftUI

/// The 34 pt send button: Send, Steer, Queue, Stop or Stopping, dimmed where the agent can't take the action.
@MainActor
struct SendButton: View {
    let action: ChatModel.Action
    let empty: Bool
    let allowed: Bool
    let press: () -> Void

    var body: some View {
        Button {
            Haptics.tap()
            press()
        } label: {
            switch action {
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
        .buttonStyle(.plain)
        .disabled(action == .stopping || (action == .send && empty))
        .opacity(allowed ? 1 : 0.35)
        .accessibilityLabel(Self.label(action))
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
    let stop: () -> Void

    var body: some View {
        Button {
            Haptics.warning()
            stop()
        } label: {
            RoundedRectangle(cornerRadius: 2).fill(Palette.text2.color).frame(width: 10, height: 10)
                .frame(width: 34, height: 34)
                .overlay(Circle().strokeBorder(Palette.separatorStrong.color))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Stop")
    }
}
