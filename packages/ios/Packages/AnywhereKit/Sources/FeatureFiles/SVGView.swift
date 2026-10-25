import CodeView
import SwiftUI

#if os(iOS)
    import WebKit
#endif

/// An SVG drawn by WebKit with scripts and loads off; where WebKit isn't used, its source.
@MainActor
struct SVGView: View {
    let source: String

    var body: some View {
        #if os(iOS)
            SVGWebView(html: Self.page(source))
        #else
            CodeTextView(text: source, language: "html")
        #endif
    }

    /// The SVG centred on a page that loads nothing else and follows the reader's appearance.
    nonisolated static func page(_ svg: String) -> String {
        """
        <!doctype html><html><head>
        <meta name="viewport" content="width=device-width, initial-scale=1, maximum-scale=8">
        <meta http-equiv="Content-Security-Policy" content="default-src 'none'; img-src data:; style-src 'unsafe-inline'">
        <style>
        :root { color-scheme: light dark; }
        html, body { margin: 0; height: 100%; background: transparent; }
        body { display: flex; align-items: center; justify-content: center; }
        body > svg { max-width: 92vw; max-height: 92vh; width: auto; height: auto; }
        </style></head><body>\(svg)</body></html>
        """
    }
}

#if os(iOS)
    private struct SVGWebView: UIViewRepresentable {
        let html: String

        func makeUIView(context: Context) -> WKWebView {
            let config = WKWebViewConfiguration()
            config.defaultWebpagePreferences.allowsContentJavaScript = false
            let view = WKWebView(frame: .zero, configuration: config)
            view.isOpaque = false
            view.backgroundColor = .clear
            view.scrollView.backgroundColor = .clear
            return view
        }

        func updateUIView(_ view: WKWebView, context: Context) {
            guard context.coordinator.shown != html else { return }
            context.coordinator.shown = html
            view.loadHTMLString(html, baseURL: nil)
        }

        func makeCoordinator() -> Coordinator { Coordinator() }

        final class Coordinator {
            var shown: String?
        }
    }
#endif
