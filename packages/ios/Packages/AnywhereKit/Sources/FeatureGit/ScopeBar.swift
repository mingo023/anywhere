import DesignSystem
import SwiftUI

/// Uncommitted or Compare, and the branch Compare is against.
@MainActor
struct ScopeBar: View {
    @Binding var comparing: Bool
    let against: String
    let picked: Bool
    let choose: () -> Void
    let reset: () -> Void

    var body: some View {
        HStack(spacing: Space.s) {
            Picker("Scope", selection: $comparing) {
                Text("Uncommitted").tag(false)
                Text("vs \(against)").tag(true)
            }
            .pickerStyle(.segmented)
            Menu {
                Button(action: choose) { Label { Text("Compare Against…") } icon: { Image.named("branch", size: 16) } }
                if picked {
                    Button("Compare Against the Base", systemImage: "arrow.uturn.backward", action: reset)
                }
            } label: {
                Label("Compare Against", systemImage: "chevron.down.circle").labelStyle(.iconOnly).hitTarget()
            }
        }
        .padding(.horizontal, Space.l)
        .background(.bar)
    }
}
