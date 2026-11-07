import SwiftUI

/// Asks before Full: on a phone, skipping every approval is a tap away.
@MainActor
struct FullAccessConfirm: ViewModifier {
    @Binding var isPresented: Bool
    let who: String
    let confirm: () -> Void

    func body(content: Content) -> some View {
        content.confirmationDialog("Skip every approval?", isPresented: $isPresented, titleVisibility: .visible) {
            Button("Switch to Full", role: .destructive, action: confirm)
        } message: {
            Text("\(who) will run commands and change files without asking.")
        }
    }
}

extension View {
    /// `who`: the agent, as the confirmation names it.
    public func confirmsFullAccess(_ isPresented: Binding<Bool>, who: String, confirm: @escaping () -> Void) -> some View {
        modifier(FullAccessConfirm(isPresented: isPresented, who: who, confirm: confirm))
    }
}
