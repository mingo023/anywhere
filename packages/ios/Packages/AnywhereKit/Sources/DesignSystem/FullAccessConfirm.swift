import SwiftUI

/// Asks before Full: on a phone, skipping every approval is a tap away.
@MainActor
struct FullAccessConfirm: ViewModifier {
    @Binding var isPresented: Bool
    let who: String
    let action: String
    let confirm: () -> Void

    func body(content: Content) -> some View {
        content.confirmationDialog("Skip every approval?", isPresented: $isPresented, titleVisibility: .visible) {
            Button(action, role: .destructive, action: confirm)
        } message: {
            Text("\(who) will run commands and change files without asking.")
        }
    }
}

extension View {
    /// `who`: the agent, as the confirmation names it. `action`: what confirming does, as its button reads.
    public func confirmsFullAccess(
        _ isPresented: Binding<Bool>, who: String, action: String = "Switch to Full", confirm: @escaping () -> Void
    ) -> some View {
        modifier(FullAccessConfirm(isPresented: isPresented, who: who, action: action, confirm: confirm))
    }
}
