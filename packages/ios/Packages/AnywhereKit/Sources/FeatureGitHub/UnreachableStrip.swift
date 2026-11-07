import DesignSystem
import SwiftUI

/// A strip over a PR gh couldn't refresh: it shows what gh last said, with Retry.
@MainActor
struct UnreachableStrip: View {
    let retry: () -> Void

    var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "exclamationmark.icloud").imageScale(.small)
            Text("Couldn't reach GitHub · showing the last answer").lineLimit(1)
            Spacer(minLength: 0)
            Button(action: retry) { Text("Retry").fontWeight(.semibold).hitTarget() }.buttonStyle(.borderless)
        }
        .font(Typography.footnote.weight(.medium))
        .foregroundStyle(Palette.waitingText.color)
        .padding(.horizontal, Space.l)
        .frame(maxWidth: .infinity)
        .background(Palette.waitingBackground.color)
    }
}
