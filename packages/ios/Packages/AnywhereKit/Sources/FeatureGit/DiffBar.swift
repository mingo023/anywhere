import DesignSystem
import SwiftUI

/// Under a diff: the previous and next file together, and Stage or Unstage, so the review of a file ends there.
@MainActor
struct DiffBar: View {
    let previous: (() -> Void)?
    let next: (() -> Void)?
    /// "Stage" or "Unstage"; nil for a diff that can't be staged.
    let stage: String?
    let staging: () -> Void
    /// Stage is on; not while the Mac is offline.
    let writable: Bool

    var body: some View {
        HStack(spacing: Space.s) {
            HStack(spacing: 0) {
                Button { previous?() } label: { Label("Previous File", systemImage: "chevron.up").labelStyle(.iconOnly).hitTarget() }
                    .disabled(previous == nil)
                Button { next?() } label: { Label("Next File", systemImage: "chevron.down").labelStyle(.iconOnly).hitTarget() }
                    .disabled(next == nil)
            }
            .buttonStyle(.plain)
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, Space.xs)
            .glass(in: Capsule())
            Spacer(minLength: 0)
            if let stage {
                Button(stage, action: staging)
                    .buttonStyle(.plain)
                    .font(Typography.body.weight(.semibold))
                    .foregroundStyle(Palette.text.color)
                    .padding(.horizontal, Space.l)
                    .frame(minHeight: HitTarget.minimum)
                    .glass(in: Capsule())
                    .disabled(!writable)
            }
        }
        .padding(.horizontal, Space.l)
        .padding(.bottom, Space.xs)
    }
}
