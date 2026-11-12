import DesignSystem
import Observation
import SwiftUI

#if os(iOS)
    import UIKit
#endif

/// A button a row shows when swiped from one edge, as `swipeActions` does in a `List`.
struct SwipeAction {
    let title: String
    let systemImage: String
    let tint: Color
    let action: () -> Void
}

/// The one row of a list that shows its swipe buttons, so opening another or scrolling shuts it.
@MainActor
@Observable
final class SwipeOpen {
    fileprivate var id: String?
    fileprivate var edge: HorizontalEdge = .leading

    func close() { id = nil }
}

extension View {
    /// Scrolling shuts the open row, as it does in a `List`.
    @MainActor @ViewBuilder func closesSwipe(_ open: SwipeOpen) -> some View {
        #if os(iOS)
            onScrollPhaseChange { _, phase in
                if phase == .interacting, open.id != nil { withAnimation(Swipe.spring) { open.close() } }
            }
        #else
            self
        #endif
    }

    /// Swipe from the leading edge for `leading`, from the trailing edge for `trailing`; a full swipe runs it. Rows of a
    /// lazy stack can't use `swipeActions`, which only a `List` draws.
    @MainActor func swipe(_ id: String, open: SwipeOpen, leading: SwipeAction?, trailing: SwipeAction?) -> some View {
        modifier(Swipe(id: id, open: open, leading: leading, trailing: trailing))
    }
}

@MainActor
private struct Swipe: ViewModifier {
    let id: String
    let open: SwipeOpen
    let leading: SwipeAction?
    let trailing: SwipeAction?

    static let button: CGFloat = 80
    /// Past this a swipe runs the action without a tap, as a full swipe does.
    static let full: CGFloat = 200
    static let spring = Animation.spring(duration: 0.3, bounce: 0.1)

    #if os(iOS)
        @State private var drag: CGFloat = 0

        func body(content: Content) -> some View {
            let x = offset(drag)
            content
                .offset(x: x)
                .background(alignment: x > 0 ? .leading : .trailing) {
                    if x > 0, let leading { button(leading, width: x) } else if x < 0, let trailing { button(trailing, width: -x) }
                }
                .overlay {
                    if open.id == id { Color.clear.contentShape(Rectangle()).onTapGesture { settle { open.close() } } }
                }
                .clipped()
                // `simultaneousGesture` takes no UIKit recognizer.
                .gesture(SidewaysPan(began: closeOthers, moved: { drag = $0 }, ended: ended, cancelled: { settle { drag = 0 } }))
                .accessibilityActions {
                    if let leading { Button(leading.title, action: leading.action) }
                    if let trailing { Button(trailing.title, action: trailing.action) }
                }
        }

        private var rest: CGFloat {
            guard open.id == id else { return 0 }
            return open.edge == .leading ? Self.button : -Self.button
        }

        private func offset(_ drag: CGFloat) -> CGFloat {
            let x = rest + drag
            return min(max(x, trailing == nil ? 0 : -.infinity), leading == nil ? 0 : .infinity)
        }

        private func closeOthers() {
            if open.id != nil, open.id != id { settle { open.close() } }
        }

        private func ended(_ translation: CGFloat) {
            let x = offset(translation)
            settle {
                drag = 0
                if x >= Self.full, let leading {
                    open.close()
                    leading.action()
                } else if x <= -Self.full, let trailing {
                    open.close()
                    trailing.action()
                } else if x > Self.button / 2 {
                    open.id = id
                    open.edge = .leading
                } else if x < -Self.button / 2 {
                    open.id = id
                    open.edge = .trailing
                } else if open.id == id {
                    open.close()
                }
            }
        }

        private func button(_ a: SwipeAction, width: CGFloat) -> some View {
            Button {
                settle { open.close() }
                a.action()
            } label: {
                Label(a.title, systemImage: a.systemImage)
                    .labelStyle(.iconOnly)
                    .font(.title3)
                    .foregroundStyle(.white)
                    .frame(width: width)
                    .frame(maxHeight: .infinity)
                    .background(a.tint)
            }
            .buttonStyle(.plain)
        }

        private func settle(_ change: () -> Void) {
            withAnimation(Swipe.spring, change)
        }
    #else
        func body(content: Content) -> some View { content }
    #endif
}

#if os(iOS)
    /// A pan that starts only when the finger moves more sideways than up or down, and reports how far across. A SwiftUI
    /// drag on a row of a scroll view holds up the scroll and leaves the row's link pressed, so a vertical drag that
    /// starts on a row opens it instead of scrolling.
    private struct SidewaysPan: UIGestureRecognizerRepresentable {
        let began: () -> Void
        let moved: (CGFloat) -> Void
        let ended: (CGFloat) -> Void
        /// The system took the touch (a multitasking gesture, a call); nothing is run.
        let cancelled: () -> Void

        func makeCoordinator(converter: CoordinateSpaceConverter) -> Coordinator { Coordinator() }

        func makeUIGestureRecognizer(context: Context) -> UIPanGestureRecognizer {
            let pan = UIPanGestureRecognizer()
            pan.delegate = context.coordinator
            return pan
        }

        func handleUIGestureRecognizerAction(_ recognizer: UIPanGestureRecognizer, context: Context) {
            let x = recognizer.translation(in: recognizer.view).x
            switch recognizer.state {
            case .began:
                began()
                moved(x)
            case .changed: moved(x)
            case .ended: ended(x)
            case .cancelled, .failed: cancelled()
            default: break
            }
        }

        final class Coordinator: NSObject, UIGestureRecognizerDelegate {
            func gestureRecognizerShouldBegin(_ recognizer: UIGestureRecognizer) -> Bool {
                guard let pan = recognizer as? UIPanGestureRecognizer else { return false }
                let v = pan.velocity(in: pan.view)
                return abs(v.x) > abs(v.y)
            }
        }
    }
#endif
