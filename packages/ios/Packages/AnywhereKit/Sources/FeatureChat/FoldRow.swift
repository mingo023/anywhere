import DesignSystem
import SwiftUI

/// The line heading a settled reply, "Claude worked for 1m 4s"; a tap opens or shuts its work.
@MainActor
struct FoldRow: View {
    let label: String
    let open: Bool
    /// The reply just settled: its label fades in.
    let settled: Bool
    let provider: String
    let toggle: () -> Void
    @State private var shown = true

    var body: some View {
        Button {
            Haptics.tap()
            toggle()
        } label: {
            HStack(spacing: 6) {
                if open {
                    LucideIcon(.chevronDown, size: 13).frame(width: 14)
                } else {
                    BrandMark(provider).frame(width: 14)
                }
                Text(label)
                if !open { LucideIcon(.chevronRight, size: 12) }
            }
            .font(ChatFont.fold)
            .foregroundStyle(Palette.text3.color)
            .frame(minHeight: 28)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .opacity(shown ? 1 : 0)
        .onChange(of: settled, initial: true) { _, settled in
            guard settled else { return }
            shown = false
            withAnimation(.easeOut(duration: 0.24)) { shown = true }
        }
        .accessibilityHint(open ? "Hides the work" : "Shows the work")
    }
}

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
