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
    /// Opens the screen of the sub-agent a call spawned.
    var openAgent: (_ callId: String) -> Void = { _ in }
    /// A link the transcript or the composer names: a file, or a web page.
    let openLink: (MarkdownLink) -> Void
    /// Off for gallery scenes, which have no Mac to tell.
    var live = true
    @State private var mapOpen = false
    @State private var showing: String?
    /// Taps on the transcript, which close the suggest list.
    @State private var outsideTaps = 0

    var body: some View {
        let _ = BodyCount.hit("ChatScreen")
        GeometryReader { screen in
            VStack(spacing: 0) {
                TranscriptView(store: store, showing: $showing, openLink: openLink, openAgent: openAgent)
                    .simultaneousGesture(TapGesture().onEnded { outsideTaps += 1 })
                if !store.online { StaleStrip(mac: machine) }
                ComposerView(
                    store: store, machine: machine, height: screen.size.height, outsideTaps: outsideTaps, show: show, mapOpen: $mapOpen,
                    openLink: openLink)
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
            AgentMapSheet(transcript: store.session.transcript, provider: store.provider, openAgent: openAgent)
        }
    }

    /// A tray's sub-agent opens on its own screen; a command is shown where it ran.
    private func show(_ callId: String) {
        if store.background.contains(where: { $0.id == callId && $0.agent != nil }) { openAgent(callId) } else { showing = callId }
    }
}
