import ChatModel
import DesignSystem
import SwiftUI

/// The draft at full size, for one too long for the composer's box.
@MainActor
struct ComposerEditor: View {
    @Binding var text: String
    let attach: AttachMenu.Attach
    @FocusState private var focused: Bool
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            TextEditor(text: $text)
                .font(ChatFont.bubble)
                .focused($focused)
                #if os(iOS)
                    .pastesImages(attach)
                #endif
                .scrollContentBackground(.hidden)
                .padding(.horizontal, Space.m)
                .screenBackground()
                .navigationTitle("Message")
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                #endif
                .toolbar {
                    ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                }
        }
        .onAppear { focused = true }
    }
}
