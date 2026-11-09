import DesignSystem
import SwiftUI

#if os(iOS)
    import UIKit
#else
    import AppKit
#endif

/// The bar under picked lines: which lines, Ask in Chat when the screen can send them, Copy, and a way to let go.
@MainActor
struct PickBar: View {
    let quote: CodeQuote
    /// What Copy puts on the clipboard: the lines as they read in the file.
    let copyText: String
    let ask: ((CodeQuote) -> Void)?
    let close: () -> Void
    @State private var copied = false

    var body: some View {
        HStack(spacing: Space.s) {
            Text(quote.title)
                .font(Typography.subheadline.monospacedDigit())
                .foregroundStyle(Palette.text2.color)
                .lineLimit(1)
            Spacer(minLength: Space.s)
            if let ask {
                Button("Ask in Chat", systemImage: "bubble.left") {
                    ask(quote)
                    close()
                }
                .buttonStyle(CapsuleButtonStyle())
            }
            Button(copied ? "Copied" : "Copy", systemImage: copied ? "checkmark" : "doc.on.doc", action: copy)
                .buttonStyle(.bordered)
            Button("Done", systemImage: "xmark", action: close)
                .labelStyle(.iconOnly)
                .frame(minWidth: 44, minHeight: 44)
        }
        .controlSize(.regular)
        .frame(minHeight: 44)
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.xs)
        .background(.bar)
        .overlay(alignment: .top) { Divider() }
        .task(id: copied) {
            guard copied else { return }
            try? await Task.sleep(for: .seconds(2.4))
            copied = false
        }
    }

    private func copy() {
        #if os(iOS)
            UIPasteboard.general.string = copyText
        #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(copyText, forType: .string)
        #endif
        Haptics.success()
        copied = true
    }
}
