import DesignSystem
import Domain
import SwiftUI

/// What the agent may do without asking: the access, or a shield when short of room; a picker for a chat agent pocketd lets the phone set.
/// Full asks first.
@MainActor
struct AccessPillView: View {
    let access: String
    let editable: Bool
    /// The agent, as the confirmation names it.
    let who: String
    var fit = PillFit.full
    let pick: (String) -> Void
    @State private var confirming = false

    var body: some View {
        if editable {
            Menu {
                Section("Access") {
                    AccessChoices(choices: AccessPill.choices(launching: false), picked: access) { choose($0) }
                }
            } label: {
                label
            }
            .menuStyle(.button)
            .buttonStyle(.plain)
            .fixedSize()
            .confirmsFullAccess($confirming, who: who) { pick("full") }
        } else {
            label
        }
    }

    private func choose(_ value: String) {
        if AccessPill.confirms(value, from: access) {
            Haptics.warning()
            confirming = true
        } else {
            pick(value)
        }
    }

    private var label: some View {
        let warns = AccessPill.warns(access)
        return PillLabel {
            if fit <= .short {
                Text(AccessPill.label(access)).foregroundStyle((warns ? Palette.waitingText : Palette.textBody).color)
            } else {
                LucideIcon(warns ? .shieldAlert : .shield, size: 14).foregroundStyle(Palette.text2.color)
            }
        }
        .accessibilityLabel("Access: \(AccessPill.label(access))")
    }
}
