import Fixtures
import Foundation
import Testing
import Wire
@testable import Stores

@MainActor
private final class Screen: TerminalSink {
    var calls: [String] = []
    func replace(cols: Int, rows: Int, with bytes: Data) { calls.append("replace \(cols)x\(rows) \(String(decoding: bytes, as: UTF8.self))") }
    func feed(_ bytes: Data) { calls.append("feed \(String(decoding: bytes, as: UTF8.self))") }
    func resize(cols: Int, rows: Int) { calls.append("resize \(cols)x\(rows)") }
}

@MainActor
private final class Sent {
    var messages: [ClientMessage] = []
}

@MainActor
private func store() -> (TerminalStore, Sent) {
    let sent = Sent()
    let store = TerminalStore(post: { sent.messages.append($0) })
    _ = store.apply(.terminalList(TerminalList(terminals: Sample.terminals)))
    return (store, sent)
}

private func frame(_ kind: TerminalFrame.Kind, _ id: String, _ text: String) -> TerminalFrame {
    TerminalFrame(kind: kind, terminalId: id, payload: Data(text.utf8))
}

@Suite("Terminal store")
@MainActor
struct TerminalStoreTests {
    @Test("an attach takes its size from the reply, then the snapshot replaces the screen and output follows")
    func attach() {
        let (store, sent) = store()
        let screen = Screen()
        let open = store.attach("t-shell", sink: screen)
        #expect(sent.messages == [.terminalAttach(.init(terminalId: "t-shell"))])
        #expect(open.state == .attaching)
        #expect(store.apply(.terminalAttached(.init(terminalId: "t-shell", cols: 100, rows: 30))))
        #expect(open.state == .live)
        #expect(store.apply(frame(.snapshot, "t-shell", "$ ")))
        #expect(store.apply(frame(.output, "t-shell", "ls\r\n")))
        #expect(!store.apply(frame(.output, "t-other", "x")))
        #expect(screen.calls == ["replace 100x30 $ ", "feed ls\r\n"])
    }

    @Test("a resize reaches the open screen and the list")
    func resize() {
        let (store, _) = store()
        let screen = Screen()
        store.attach("t-shell", sink: screen)
        _ = store.apply(.terminalResized(.init(terminalId: "t-shell", cols: 48, rows: 20)))
        #expect(screen.calls == ["resize 48x20"])
        #expect(store.terminal("t-shell")?.cols == 48)
        #expect(store.attachments["t-shell"]?.rows == 20)
    }

    @Test("the exit ends the screen, and frames after it are dropped")
    func exit() {
        let (store, _) = store()
        let screen = Screen()
        let open = store.attach("t-shell", sink: screen)
        _ = store.apply(.terminalExited(.init(terminalId: "t-shell", code: 3)))
        #expect(open.state == .exited(code: 3))
        #expect(!store.apply(frame(.output, "t-shell", "late")))
        #expect(screen.calls.isEmpty)
    }

    @Test("a live screen whose terminal leaves the list is gone; one still attaching is not")
    func gone() {
        let (store, _) = store()
        let live = store.attach("t-shell", sink: Screen())
        let attaching = store.attach("t-review", sink: Screen())
        _ = store.apply(.terminalAttached(.init(terminalId: "t-shell", cols: 120, rows: 40)))
        _ = store.apply(.terminalList(TerminalList(terminals: [])))
        #expect(live.state == .gone)
        #expect(attaching.state == .attaching)
        #expect(store.terminals.isEmpty)
    }

    @Test("a drop leaves the screens offline, and the reconnect lists again and attaches each afresh")
    func reconnect() {
        let (store, sent) = store()
        let open = store.attach("t-shell", sink: Screen())
        let ended = store.attach("t-review", sink: Screen())
        _ = store.apply(.terminalAttached(.init(terminalId: "t-shell", cols: 120, rows: 40)))
        _ = store.apply(.terminalExited(.init(terminalId: "t-review", code: 0)))
        sent.messages = []
        store.disconnected()
        #expect(open.state == .offline)
        #expect(!store.listed)
        store.connected(Sample.hello)
        #expect(sent.messages == [.terminalList, .terminalAttach(.init(terminalId: "t-shell"))])
        #expect(open.state == .attaching)
        #expect(ended.state == .exited(code: 0))
    }

    @Test("a Mac without terminal.v1 has no terminals to ask for")
    func unavailable() {
        let (store, sent) = store()
        store.attach("t-shell", sink: Screen())
        sent.messages = []
        var old = Sample.hello
        old.caps = [Caps.locals]
        store.connected(old)
        #expect(!store.available)
        #expect(sent.messages.isEmpty)
    }

    @Test("a detach stops the frames and tells pocketd")
    func detach() {
        let (store, sent) = store()
        let screen = Screen()
        store.attach("t-shell", sink: screen)
        store.detach("t-shell")
        #expect(!store.apply(frame(.output, "t-shell", "x")))
        #expect(sent.messages.last == .terminalDetach(.init(terminalId: "t-shell")))
        store.detach("t-shell")
        #expect(sent.messages.count == 2)
    }

    @Test("keystrokes and fits go out as typed, the size kept to what pocketd takes")
    func typing() {
        let (store, sent) = store()
        store.input("t-shell", Data("l".utf8))
        store.input("t-shell", Data())
        store.input("t-shell", Data("s".utf8))
        store.resize("t-shell", cols: 0, rows: 4000)
        #expect(sent.messages == [
            .terminalInput(.init(terminalId: "t-shell", data: Data("l".utf8))),
            .terminalInput(.init(terminalId: "t-shell", data: Data("s".utf8))),
            .terminalResize(.init(terminalId: "t-shell", cols: 1, rows: 1000)),
        ])
    }

    @Test("terminals_off leaves the screens watching")
    func writesOff() {
        let (store, _) = store()
        #expect(store.apply(.error(ServerError(message: "Terminals are off for phones on this Mac", code: .terminalsOff))))
        #expect(store.writesOff)
        #expect(!store.apply(.error(ServerError(message: "other"))))
    }

    @Test("a reconnect forgets terminals_off, as the owner may have turned terminals back on")
    func writesOnAgain() {
        let (store, _) = store()
        _ = store.apply(.error(ServerError(message: "Terminals are off for phones on this Mac", code: .terminalsOff)))
        store.disconnected()
        store.connected(Sample.hello)
        #expect(!store.writesOff)
    }

    @Test("an attach refused as ended shows the screen ended instead of connecting forever")
    func attachEnded() {
        let (store, _) = store()
        let ended = store.attach("t-shell", sink: Screen())
        let other = store.attach("t-review", sink: Screen())
        #expect(!store.apply(.error(ServerError(message: "This terminal has ended", code: .terminalEnded, replyTo: "c9"))))
        #expect(ended.state == .attaching)
        // An older pocketd refuses it without a code.
        #expect(store.apply(.error(ServerError(message: "This terminal has ended", replyTo: "c1"))))
        #expect(ended.state == .gone)
        #expect(other.state == .attaching)
        #expect(store.apply(.error(ServerError(message: "This terminal has ended", code: .terminalEnded, replyTo: "c2"))))
        #expect(other.state == .gone)
    }

    @Test("a keystroke refused while the terminal's agent asks marks the screen as waiting on the ask, until it resolves")
    func askOpen() {
        let (store, sent) = store()
        let open = store.attach("t-shell", sink: Screen())
        _ = store.apply(.terminalAttached(.init(terminalId: "t-shell", cols: 120, rows: 40)))
        store.input("t-shell", Data("y".utf8))
        #expect(sent.messages.count == 2)
        #expect(store.apply(.error(ServerError(message: "waiting on a permission", code: .askOpen, replyTo: "c2"))))
        #expect(open.askOpen)
        #expect(store.apply(.permissionResolved(PermissionResolved(requestId: "r1", decision: .allow))))
        #expect(!open.askOpen)
    }

    @Test("the Mac's terminals setting makes the screens view-only before anything is typed")
    func terminalsSetting() {
        let (store, _) = store()
        var hello = Sample.hello
        hello.host = HostState(tailnet: true, keepingAwake: false, terminals: false)
        store.connected(hello)
        #expect(store.writesOff)
        #expect(store.apply(.hostChanged(HostChanged(host: HostState(tailnet: true, keepingAwake: false, terminals: true)))))
        #expect(!store.writesOff)
        #expect(!store.apply(.hostChanged(HostChanged(host: HostState(tailnet: false, keepingAwake: false)))))
        #expect(!store.writesOff)
    }

    @Test("the Locals a terminal may open in follow local.list")
    func locals() {
        let (store, _) = store()
        #expect(store.apply(.localList(LocalList(locals: Sample.locals))))
        #expect(!store.apply(.localList(LocalList(locals: Sample.locals))))
        #expect(store.locals == Sample.locals)
    }
}
