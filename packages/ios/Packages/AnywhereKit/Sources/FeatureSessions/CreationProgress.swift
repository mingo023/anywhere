import DesignSystem
import Stores
import SwiftUI

/// The steps pocketd reported for a worktree or session being made: done ones ticked, the running one turning, and
/// what stopped it.
@MainActor
struct CreationProgress: View {
    let creation: Creation?
    let retry: () -> Void
    /// Offered beside Try Again when given.
    var dismiss: (() -> Void)?

    var body: some View {
        Section {
            let steps = creation?.steps ?? []
            if steps.isEmpty && creation?.error == nil && creation?.waiting != true {
                row("Asking your Mac", running: true)
            }
            ForEach(Array(steps.enumerated()), id: \.offset) { i, step in
                let running = i == steps.count - 1 && creation?.done != true && creation?.error == nil && creation?.waiting != true
                VStack(alignment: .leading, spacing: Space.xxs) {
                    row(SessionText.step(step.step), running: running)
                    if let note = step.note { Text(note).font(Typography.footnote).foregroundStyle(Palette.text3.color) }
                }
            }
            if let creation, creation.waiting {
                row(SessionText.creating(creation), running: true)
            }
        } header: {
            Text("Starting")
        }
        if let error = creation?.error {
            Section {
                Label(error.message, lucide: .circleAlert).foregroundStyle(Palette.failedText.color)
                if let detail = error.detail { Text(detail).font(Typography.codeSmall).foregroundStyle(Palette.text3.color) }
                Button("Try Again", action: retry)
                if let dismiss { Button("Dismiss", action: dismiss) }
            }
        }
    }

    private func row(_ text: String, running: Bool) -> some View {
        HStack(spacing: Space.m) {
            Group {
                if running {
                    DotSpinner(size: 13)
                } else {
                    LucideIcon(.circleCheck, size: 17).foregroundStyle(Palette.success.color)
                }
            }
            .frame(width: 20)
            Text(text).foregroundStyle(Palette.text.color)
        }
    }
}
