import DesignSystem
import SwiftUI
import Wire

/// A worktree's pull request on its row: its number, coloured by what it waits on.
@MainActor
struct PrBadge: View {
    let item: PrItem

    var body: some View {
        HStack(spacing: 5) {
            Circle().fill(Pill.colors(Self.tone(item)).text.color).frame(width: 6, height: 6)
            Text("#\(item.number)").font(.system(size: 12, weight: .semibold, design: .monospaced)).foregroundStyle(Palette.textBody.color)
        }
        .padding(.horizontal, 7)
        .frame(height: 22)
        .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.chip, style: .continuous))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("Pull request \(SessionText.pullRequest(item))")
    }

    /// Merged reads as done; failing checks, conflicts or requested changes as failed; running checks as working.
    nonisolated static func tone(_ item: PrItem) -> Pill.Tone {
        if item.state == "MERGED" { return .accent }
        if item.state == "CLOSED" || item.draft { return .neutral }
        if item.checks.failed > 0 || [.conflicts, .checksFailed, .changesRequested].contains(item.gate) { return .failed }
        if item.checks.pending > 0 || item.gate == .checksRunning { return .working }
        return .success
    }
}
