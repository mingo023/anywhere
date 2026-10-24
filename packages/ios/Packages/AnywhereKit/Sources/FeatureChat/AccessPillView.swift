import ChatModel
import DesignSystem
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
                ForEach(AccessPill.choices(upTo: max), id: \.value) { choice in
                    Button {
                        pick(choice.value)
                    } label: {
                        if choice.value == access {
                            Label(choice.label, systemImage: "checkmark")
                        } else {
                            Text(choice.label)
                        }
                        Text(choice.hint)
                    }
                }
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
