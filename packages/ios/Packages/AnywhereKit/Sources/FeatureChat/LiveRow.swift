import DesignSystem
import SwiftUI

/// What the agent is doing, under the reply it works on: the working dots and a label whose time ticks.
@MainActor
struct LiveRow: View {
    let live: LiveContent

    var body: some View {
        HStack(spacing: 10) {
            DotSpinner(size: 13)
            TimelineView(.periodic(from: .now, by: 1)) { context in
                Text(live.label(now: Int(context.date.timeIntervalSince1970 * 1000))).monospacedDigit()
            }
        }
        .font(.system(size: 15))
        .foregroundStyle(Palette.text2.color)
        .frame(minHeight: 28)
        .accessibilityAddTraits(.updatesFrequently)
    }
}
