#if os(iOS)
    import SwiftUI

    extension View {
        /// Adds pasting an image to the edit menu of the text field this is on, where the system offers Paste only for text.
        /// `pasteDestination` needs iOS 27.
        func pastesImages(_ attach: @escaping AttachMenu.Attach) -> some View {
            background(ImagePasteHook(attach: attach))
        }
    }

    private struct ImagePasteHook: UIViewRepresentable {
        let attach: AttachMenu.Attach

        func makeCoordinator() -> ImagePasteMenu { ImagePasteMenu() }

        func makeUIView(context: Context) -> UIView {
            let view = UIView()
            view.isUserInteractionEnabled = false
            return view
        }

        func updateUIView(_ view: UIView, context: Context) {
            context.coordinator.attach = attach
            // The field's text view isn't in the window yet during the first update.
            DispatchQueue.main.async { context.coordinator.hook(near: view) }
        }
    }

    /// Stands in as the text view's delegate for the edit menu alone, passing everything else to SwiftUI's.
    @MainActor
    private final class ImagePasteMenu: NSObject, UITextViewDelegate {
        var attach: AttachMenu.Attach = { _, _, _ in }
        private weak var field: UITextView?
        nonisolated(unsafe) private weak var swiftUI: (any UITextViewDelegate)?

        /// Takes the text view under `view`, whose centre it shares; a window can hold the transcript's text views too.
        func hook(near view: UIView) {
            if let field, field.window != nil, field.delegate === self { return }
            guard let window = view.window else { return }
            let center = view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: window)
            var root = view.superview
            while let at = root {
                if let found = Self.textView(in: at, at: center) {
                    if found.delegate !== self { swiftUI = found.delegate }
                    found.delegate = self
                    field = found
                    return
                }
                root = at.superview
            }
        }

        private static func textView(in view: UIView, at center: CGPoint) -> UITextView? {
            if let text = view as? UITextView, text.convert(text.bounds, to: nil).contains(center) { return text }
            return view.subviews.lazy.compactMap { textView(in: $0, at: center) }.first
        }

        func textView(_ textView: UITextView, editMenuForTextIn range: NSRange, suggestedActions: [UIMenuElement]) -> UIMenu? {
            let menu = swiftUI?.textView?(textView, editMenuForTextIn: range, suggestedActions: suggestedActions)
            let board = UIPasteboard.general
            guard board.hasImages else { return menu }
            let attach = attach
            let paste = UIAction(title: board.hasStrings || board.hasURLs ? "Paste Image" : "Paste") { _ in AttachMenu.pasteClipboard(to: attach) }
            return UIMenu(children: [paste] + (menu?.children ?? suggestedActions))
        }

        override func responds(to selector: Selector!) -> Bool {
            super.responds(to: selector) || swiftUI?.responds(to: selector) == true
        }

        override func forwardingTarget(for selector: Selector!) -> Any? { swiftUI }
    }
#endif
