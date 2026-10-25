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

    /// Text to select and copy from, as iOS apps offer it for content that can't be selected in place.
    public struct SelectTextSheet: View {
        let text: String
        @Environment(\.dismiss) private var dismiss

        public init(_ text: String) { self.text = text }

        public var body: some View {
            NavigationStack {
                SelectableTextView(text: text)
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

        func makeUIView(context: Context) -> UITextView {
            let view = UITextView()
            view.isEditable = false
            view.font = .preferredFont(forTextStyle: .body)
            view.adjustsFontForContentSizeCategory = true
            view.textContainerInset = UIEdgeInsets(top: 16, left: 12, bottom: 16, right: 12)
            return view
        }

        func updateUIView(_ view: UITextView, context: Context) {
            if view.text != text { view.text = text }
        }
    }
#endif
