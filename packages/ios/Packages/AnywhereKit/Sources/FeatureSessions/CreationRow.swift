import DesignSystem
import Stores
import SwiftUI

/// A session being started, listed where it will run until it does: what it is and its step, or what stopped it with
/// Try Again and Dismiss.
@MainActor
struct CreationRow: View {
    let creation: Creation
    let retry: () -> Void
    let dismiss: () -> Void

    var body: some View {
        HStack(spacing: Space.m) {
            Group {
                if creation.error != nil {
                    LucideIcon(.circleAlert, size: 17).foregroundStyle(Palette.failed.color)
                } else {
                    DotSpinner(size: 13)
                }
            }
            .frame(width: 18)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(creation.title.isEmpty ? "New session" : creation.title)
                    .font(Typography.body.weight(.semibold)).foregroundStyle(Palette.text.color).lineLimit(1)
                Text(creation.error?.message ?? SessionText.creating(creation))
                    .font(Typography.footnote).foregroundStyle((creation.error == nil ? Palette.text3 : Palette.failedText).color).lineLimit(2)
            }
            Spacer(minLength: Space.s)
            if creation.error != nil {
                Button(action: retry) { Text("Try Again").hitTarget() }.font(Typography.footnote.weight(.semibold))
                Button { dismiss() } label: { LucideIcon(.close, size: 13).hitTarget() }
                    .accessibilityLabel("Dismiss")
            }
        }
        .buttonStyle(.borderless)
        .padding(.vertical, Space.xxs)
        .contentShape(Rectangle())
        .accessibilityElement(children: .contain)
    }
}
