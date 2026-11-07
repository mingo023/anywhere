import ChatModel
import DesignSystem
import MarkdownView
import Stores
import SwiftUI

/// The chat over a store: the transcript, an offline strip while the Mac is away, and the composer under it.
@MainActor
struct ChatScreen: View {
    let store: ChatStore
    let machine: String
    /// A link the transcript or the composer names: a file, or a web page.
    let openLink: (MarkdownLink) -> Void
    /// Off for gallery scenes, which have no Mac to tell.
    var live = true
    @State private var mapOpen = false
    @State private var showing: String?
    /// Taps on the transcript, which close the suggest list.
    @State private var outsideTaps = 0
    @FocusState private var typing: Bool

    var body: some View {
        let _ = BodyCount.hit("ChatScreen")
        GeometryReader { screen in
            VStack(spacing: 0) {
                TranscriptView(store: store, showing: $showing, openLink: openLink)
                    .simultaneousGesture(
                        TapGesture().onEnded {
                            outsideTaps += 1
                            typing = false
                        }
                    )
                    .simultaneousGesture(
                        DragGesture(minimumDistance: 6).onChanged { drag in
                            if typing && Composer.hidesKeyboard(dragged: drag.translation.height) { typing = false }
                        }
                    )
                if !store.online { StaleStrip(mac: machine) }
                ComposerView(
                    store: store, machine: machine, height: screen.size.height, outsideTaps: outsideTaps, show: { showing = $0 }, mapOpen: $mapOpen,
                    openLink: openLink, focused: $typing)
            }
            .coordinateSpace(name: ComposerView.space)
        }
        .screenBackground()
        .navigationTitle(store.summary?.title ?? "Chat")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .onAppear {
            guard live else { return }
            store.appear()
            PushCenter.shared.visibleAgent = store.agentId
        }
        .onDisappear {
            guard live else { return }
            store.disappear()
            if PushCenter.shared.visibleAgent == store.agentId { PushCenter.shared.visibleAgent = nil }
        }
        .sheet(isPresented: $mapOpen) {
            AgentMapSheet(transcript: store.session.transcript, provider: store.provider, show: { showing = $0 }, openLink: openLink)
        }
    }
}
