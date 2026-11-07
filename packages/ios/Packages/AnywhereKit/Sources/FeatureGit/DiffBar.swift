import DesignSystem
import SwiftUI

/// Under a diff: Discard at one end, the previous and next change with where it is between, and Stage or Unstage at
/// the other end, so the review of a file ends there.
@MainActor
struct DiffBar: View {
    let changes: Int
    /// The change shown, an index into the diff's changes.
    let current: Int?
    let step: (Int) -> Void
    /// "Stage" or "Unstage"; nil for a diff that can't be staged.
    let stage: String?
    let staging: () -> Void
    let discard: (() -> Void)?

    var body: some View {
        HStack(spacing: Space.s) {
            if let discard {
                Button(role: .destructive, action: discard) { Text("Discard…").hitTarget() }
            }
            Spacer(minLength: 0)
            Button { step(-1) } label: { Label("Previous Change", systemImage: "arrow.up").labelStyle(.iconOnly).hitTarget() }
                .disabled((current ?? 0) <= 0)
            Text(GitText.position(current, of: changes))
                .font(Typography.footnote)
                .monospacedDigit()
                .foregroundStyle(Palette.text2.color)
                .lineLimit(1)
            Button { step(1) } label: { Label("Next Change", systemImage: "arrow.down").labelStyle(.iconOnly).hitTarget() }
                .disabled((current ?? -1) + 1 >= changes)
            Spacer(minLength: 0)
            if let stage {
                Button(stage, action: staging).buttonStyle(.borderedProminent)
            }
        }
        .padding(.horizontal, Space.l)
        .background(.bar)
    }
}
