import DesignSystem
import Observation
import SwiftUI

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

    #if os(iOS)
        @State private var drag: CGFloat = 0
        @State private var axis: Axis?

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
                .simultaneousGesture(gesture)
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

        private var gesture: some Gesture {
            DragGesture(minimumDistance: 12)
                .onChanged { v in
                    if axis == nil {
                        axis = abs(v.translation.width) > abs(v.translation.height) ? .horizontal : .vertical
                        // Scrolling the list, or swiping another row, shuts the open one.
                        if open.id != nil, open.id != id || axis == .vertical { settle { open.close() } }
                    }
                    guard axis == .horizontal else { return }
                    drag = v.translation.width
                }
                .onEnded { v in
                    defer { axis = nil }
                    guard axis == .horizontal else { return }
                    let x = offset(v.translation.width)
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
            withAnimation(.spring(duration: 0.3, bounce: 0.1), change)
        }
    #else
        func body(content: Content) -> some View { content }
    #endif
}
