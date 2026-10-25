import Domain
import SwiftUI

/// The access menu's items: each over its hint, or over why the phone can't pick it.
@MainActor
public struct AccessChoices: View {
    let choices: [AccessPill.Choice]
    let picked: String
    let pick: (String) -> Void

    public init(choices: [AccessPill.Choice], picked: String, pick: @escaping (String) -> Void) {
        self.choices = choices
        self.picked = picked
        self.pick = pick
    }

    public var body: some View {
        // Buttons, not a Picker: only a button's second text shows as the item's subtitle.
        ForEach(choices, id: \.value) { choice in
            Button {
                pick(choice.value)
            } label: {
                if choice.value == picked {
                    Label(choice.label, systemImage: "checkmark")
                } else {
                    Text(choice.label)
                }
                Text(choice.locked ?? choice.hint)
            }
            .disabled(choice.locked != nil)
        }
    }
}
