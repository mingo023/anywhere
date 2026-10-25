import DesignSystem
import Domain
import SwiftUI

/// What the agent may do without asking: a shield and the access, a picker for a chat agent pocketd lets the phone set.
@MainActor
struct AccessPillView: View {
    let access: String
    let editable: Bool
    let max: String
    let pick: (String) -> Void

    var body: some View {
        if editable {
            Menu {
                AccessChoices(choices: AccessPill.choices(max: max, launching: false), picked: access, pick: pick)
            } label: {
                label
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
        } else {
            label
        }
    }

    private var label: some View {
        HStack(spacing: 4) {
            Image(systemName: AccessPill.warns(access) ? "exclamationmark.shield" : "shield").font(.system(size: 11))
            Text(AccessPill.label(access))
        }
        .font(ChatFont.caption)
        .foregroundStyle((AccessPill.warns(access) ? Palette.waitingText : Palette.text3).color)
        .padding(.horizontal, 8)
        .frame(height: 28)
        .contentShape(Rectangle())
        .accessibilityLabel("Access: \(AccessPill.label(access))")
    }
}
