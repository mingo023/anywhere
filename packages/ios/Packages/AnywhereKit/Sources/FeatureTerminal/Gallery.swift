import Client
import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum TerminalGallery {
    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let tag = dark ? "dark" : "light"
            return [
                GalleryScene("terminals-list-\(tag)", dark: dark) { AnyView(NavigationStack { TerminalListView(host: host()) }) },
                GalleryScene("terminal-shell-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-shell", bytes: Sample.shellScreen, cols: 100, rows: 30, title: "zsh"))
                },
                GalleryScene("terminal-claude-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-a-working", bytes: Sample.claudeScreen, cols: 60, rows: 30, title: "claude", fontSize: 10))
                },
                GalleryScene("terminal-offline-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-shell", bytes: Sample.shellScreen, cols: 100, rows: 30, title: "zsh", offline: true))
                },
                GalleryScene("terminal-new-\(tag)", dark: dark) {
                    let store = host().terminals
                    return AnyView(NewTerminalSheet(store: store, projects: Sample.projects) { _ in })
                },
                GalleryScene("terminals-empty-\(tag)", dark: dark) {
                    let h = host()
                    h.apply([.message(.terminalList(TerminalList(terminals: [])))])
                    return AnyView(NavigationStack { TerminalListView(host: h) })
                },
            ]
        }
    }

    @MainActor static func host() -> HostModel {
        let h = HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "d1", macName: "Mingo's MacBook Pro", clientId: "gallery"))
        h.apply([.connected(Sample.hello)] + Sample.snapshot.map(ConnectionEvent.message) + [
            .message(.terminalList(TerminalList(terminals: Sample.terminals.filter { $0.exited != true }))),
        ])
        return h
    }

    @MainActor static func screen(id: String, bytes: Data, cols: Int, rows: Int, title: String, fontSize: CGFloat = 9, offline: Bool = false) -> some View {
        let store = host().terminals
        let session = TerminalSession(id: id, cols: cols, rows: rows, fontSize: fontSize)
        store.attach(id, sink: session)
        _ = store.apply(.terminalAttached(TerminalAttached(terminalId: id, cols: cols, rows: rows)))
        _ = store.apply(TerminalFrame(kind: .snapshot, terminalId: id, payload: bytes))
        if offline { store.disconnected() }
        return NavigationStack { TerminalScreenView(store: store, session: session, title: title) }
    }
}
