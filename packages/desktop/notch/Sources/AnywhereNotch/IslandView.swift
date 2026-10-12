import SwiftUI

/// The black island hanging from the notch: compact with the provider and how the ask stands, open with the ask and its answers.
struct IslandView: View {
    @ObservedObject var controller: Controller

    var body: some View {
        let island = controller.island
        let open = island.current != nil && (island.stage == .open || island.stage == .reply)
        let ear: CGFloat = island.current == nil ? 0 : open ? 14 : 6
        content
            .padding(.horizontal, ear)
            .background(Color.black)
            .clipShape(IslandShape(radius: open ? 22 : 10, ear: ear))
            .onHover { controller.hover($0) }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
    }

    /// The new content fades in as the shape grows to fit it; the old leaves at once, so the shape tracks only the new.
    private var fade: AnyTransition {
        .asymmetric(insertion: .opacity.animation(.easeOut(duration: 0.2).delay(0.1)), removal: .identity)
    }

    @ViewBuilder private var content: some View {
        let (island, notch) = (controller.island, controller.notch)
        if let ask = island.current {
            switch island.stage {
            case .open, .reply:
                Expanded(controller: controller, ask: ask, replying: island.stage == .reply).transition(fade)
            case .compact, .done:
                Compact(ask: ask, stage: island.stage, count: island.asks.count, width: max(notch.width, 140) + 2 * 92, height: notch.height)
                    .contentShape(Rectangle())
                    .onTapGesture { controller.change { $0.open() } }
                    .transition(fade)
            }
        } else {
            // Hidden by the notch itself, which the island grows out of and shrinks back into.
            Color.clear.frame(width: notch.width, height: notch.height)
        }
    }
}
