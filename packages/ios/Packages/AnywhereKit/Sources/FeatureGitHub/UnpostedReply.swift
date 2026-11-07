import DesignSystem
import SwiftUI

/// A reply that didn't post, under its thread: tap to retry, or edit or delete it.
@MainActor
struct UnpostedReply: View {
    let text: String
    let retry: () -> Void
    let edit: () -> Void
    let delete: () -> Void

    var body: some View {
        HStack(spacing: Space.xs) {
            Button {
                Haptics.tap()
                retry()
            } label: {
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(text).font(Typography.body).foregroundStyle(Palette.text.color).opacity(0.65).lineLimit(4)
                    Text("Not posted · Tap to retry").font(Typography.caption).foregroundStyle(Palette.failedText.color)
                }
                .frame(maxWidth: .infinity, minHeight: HitTarget.minimum, alignment: .leading)
                .contentShape(Rectangle())
            }
            Button(action: edit) { Image(systemName: "pencil").hitTarget() }
                .accessibilityLabel("Edit")
            Button(role: .destructive, action: delete) { Image(systemName: "trash").hitTarget() }
                .accessibilityLabel("Delete")
        }
        .buttonStyle(.borderless)
    }
}
