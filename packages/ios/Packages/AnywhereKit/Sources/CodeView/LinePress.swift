import SwiftUI

#if os(iOS)
    import UIKit

    /// A long press that reports where it lands and where the finger moves after, so a row can be picked and the pick
    /// stretched by dragging. SwiftUI's long press tells neither.
    struct LinePress: UIGestureRecognizerRepresentable {
        let began: (CGPoint) -> Void
        let moved: (CGPoint) -> Void
        let ended: () -> Void

        func makeUIGestureRecognizer(context: Context) -> UILongPressGestureRecognizer {
            let press = UILongPressGestureRecognizer()
            press.minimumPressDuration = 0.4
            return press
        }

        func handleUIGestureRecognizerAction(_ recognizer: UILongPressGestureRecognizer, context: Context) {
            let at = context.converter.localLocation
            switch recognizer.state {
            case .began: began(at)
            case .changed: moved(at)
            case .ended, .cancelled, .failed: ended()
            default: break
            }
        }
    }
#endif

extension View {
    /// Picks rows with a long press and a drag after it, on iOS.
    @ViewBuilder
    func onLinePress(began: @escaping (CGPoint) -> Void, moved: @escaping (CGPoint) -> Void, ended: @escaping () -> Void) -> some View {
        #if os(iOS)
            gesture(LinePress(began: began, moved: moved, ended: ended))
        #else
            self
        #endif
    }
}
