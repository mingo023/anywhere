import SwiftUI

extension View {
    /// Selectable in place on the Mac only. On iOS each selectable `Text` builds a text interaction and its gesture
    /// recognizers as it scrolls on screen, a third of a long document's scroll; there, `SelectTextSheet` selects.
    public func selectableOnMac() -> some View {
        #if os(macOS)
            textSelection(.enabled)
        #else
            self
        #endif
    }
}

#if os(iOS)
    import UIKit

    /// Text to select, find in and copy from, as iOS apps offer it for content that can't be selected in place;
    /// `monospaced` for code, so its columns line up.
    public struct SelectTextSheet: View {
        let text: String
        let monospaced: Bool
        @Environment(\.dismiss) private var dismiss

        public init(_ text: String, monospaced: Bool = false) {
            self.text = text
            self.monospaced = monospaced
        }

        public var body: some View {
            NavigationStack {
                SelectableTextView(text: text, monospaced: monospaced)
                    .navigationTitle("Select Text")
                    .navigationBarTitleDisplayMode(.inline)
                    .toolbar {
                        ToolbarItem(placement: .confirmationAction) { Button("Done") { dismiss() } }
                    }
            }
        }
    }

    /// A read-only `UITextView`: it lays out only what shows, so a long file opens at once.
    private struct SelectableTextView: UIViewRepresentable {
        let text: String
        let monospaced: Bool

        func makeUIView(context: Context) -> UITextView {
            let view = UITextView()
            view.isEditable = false
            view.isFindInteractionEnabled = true
            // 17 pt is body at the default size; the metrics scale it to the reader's.
            view.font = monospaced
                ? UIFontMetrics(forTextStyle: .body).scaledFont(for: .monospacedSystemFont(ofSize: 17, weight: .regular))
                : .preferredFont(forTextStyle: .body)
            view.adjustsFontForContentSizeCategory = true
            view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 16, right: 12)
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            if view.text != text { view.text = text }
        }
    }
#endif
