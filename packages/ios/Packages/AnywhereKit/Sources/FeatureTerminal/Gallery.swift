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
                GalleryScene("terminal-ended-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-shell", bytes: Data(), cols: 100, rows: 30, title: "zsh") { store in
                        // The attach the screen sent, c1, refused: the terminal ended before the phone opened it.
                        _ = store.apply(.error(ServerError(message: "This terminal has ended", code: .terminalEnded, replyTo: "c1")))
                    })
                },
                GalleryScene("terminal-ask-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-a-working", bytes: Sample.claudeScreen, cols: 60, rows: 30, title: "claude", fontSize: 10) { store in
                        store.input("t-a-working", Data("y".utf8))
                        _ = store.apply(.error(ServerError(message: "This terminal's session is waiting on a permission", code: .askOpen,
                                                           replyTo: "c2")))
                    })
                },
                GalleryScene("terminal-viewonly-\(tag)", dark: dark) {
                    AnyView(screen(id: "t-shell", bytes: Sample.shellScreen, cols: 100, rows: 30, title: "zsh") { store in
                        _ = store.apply(.hostChanged(HostChanged(host: HostState(tailnet: true, keepingAwake: false, terminals: false))))
                    })
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

    /// A screen whose attach is c1, answered unless `then` refuses it; `then` runs after the snapshot.
    @MainActor static func screen(id: String, bytes: Data, cols: Int, rows: Int, title: String, fontSize: CGFloat = 9, offline: Bool = false,
                                  then: (TerminalStore) -> Void = { _ in }) -> some View {
        let store = TerminalStore(post: { _ in })
        _ = store.apply(.terminalList(TerminalList(terminals: Sample.terminals.filter { $0.exited != true })))
        let session = TerminalSession(id: id, cols: cols, rows: rows, fontSize: fontSize)
        store.attach(id, sink: session)
        if !bytes.isEmpty {
            _ = store.apply(.terminalAttached(TerminalAttached(terminalId: id, cols: cols, rows: rows)))
            _ = store.apply(TerminalFrame(kind: .snapshot, terminalId: id, payload: bytes))
        }
        then(store)
        if offline { store.disconnected() }
        return NavigationStack { TerminalScreenView(store: store, session: session, title: title, mac: Sample.hello.serverId) }
    }
}
