import ChatModel
import DesignSystem
import SwiftUI

/// What waits behind the turn, numbered in the order it goes, each with Steer, Edit and Remove. Once the user
/// stopped the turn, or it failed, nothing goes until Resume.
@MainActor
struct QueueTray: View {
    let sends: [Pending]
    let paused: Bool
    let canSteer: Bool
    let resume: () -> Void
    let steer: (Int) -> Void
    let edit: (Int) -> Void
    let remove: (Int) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            HStack(spacing: 8) {
                Image(systemName: "list.bullet").font(.system(size: 12))
                Text("\(sends.count) queued").fontWeight(.semibold).foregroundStyle(Palette.text.color)
                Text(paused ? "paused" : "send in order after this turn")
                Spacer(minLength: 0)
                if paused {
                    Button(action: resume) { Label("Resume", systemImage: "play.fill").frame(minHeight: 28) }
                        .buttonStyle(.plain)
                        .accessibilityHint("Sends the queued messages in order")
                }
            }
            .font(ChatFont.caption)
            .foregroundStyle(Palette.text2.color)
            ForEach(Array(sends.enumerated()), id: \.element.id) { n, p in
                HStack(spacing: 8) {
                    Text("\(n + 1)").font(.system(size: 10, weight: .semibold)).monospacedDigit()
                        .frame(width: 18, height: 18).background(Palette.fill3.color, in: Circle())
                    Text(Sticky.oneLine(p.text)).font(ChatFont.small).foregroundStyle(Palette.textBody.color).lineLimit(1)
                    Spacer(minLength: 0)
                    if canSteer {
                        Button("Steer") { steer(p.id) }.font(ChatFont.caption.weight(.medium)).accessibilityHint("Sends it into the turn underway")
                    }
                    Button { edit(p.id) } label: { Image(systemName: "pencil").frame(width: 28, height: 28) }.accessibilityLabel("Edit queued message")
                    Button { remove(p.id) } label: { Image(systemName: "xmark").frame(width: 28, height: 28) }.accessibilityLabel("Remove queued message")
                }
                .buttonStyle(.plain)
                .foregroundStyle(Palette.text3.color)
            }
        }
        .trayCard()
    }
}

extension View {
    /// A tray stacked over the composer box.
    func trayCard() -> some View {
        padding(.horizontal, Space.m)
            .padding(.vertical, Space.s)
            .background(Palette.surface.color, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 14, style: .continuous).strokeBorder(Palette.hairline.color))
            .transition(.opacity)
    }
}
