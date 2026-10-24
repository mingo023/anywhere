import Client
import Foundation
import Stores
import Testing
@testable import FeaturePairing

private let code = "q3xYq3xYq3xYq3xYq3xY_-"

@Suite("Pairing entry")
struct PairingEntryTests {
    @Test("a scanned or pasted pair link is read whole, name included")
    func link() {
        let link = PairingEntry.link(from: "  anywhere://pair?v=1&h=100.64.0.1%3A4517&c=\(code)&n=Studio\n")
        #expect(link == PairingLink(host: "100.64.0.1:4517", code: code, name: "Studio"))
    }

    @Test("the line pocketd pair prints gives its host and code")
    func pocketdLine() {
        let line = "Scan with the iPhone Camera app, or enter  100.64.0.1:4517  \(code)  Expires at 15:04."
        #expect(PairingEntry.link(from: line) == PairingLink(host: "100.64.0.1:4517", code: code, name: "100.64.0.1:4517"))
    }

    @Test("a pair link inside other text is found")
    func linkInText() {
        #expect(PairingEntry.link(from: "Open anywhere://pair?v=1&h=127.0.0.1:4518&c=\(code) on your phone")?.host == "127.0.0.1:4518")
    }

    @Test("text without both a host and a code is no link", arguments: ["", "hello", code, "100.64.0.1:4517", "anywhere://pair?v=2&h=1.2.3.4:1&c=\(code)"])
    func refuses(_ text: String) {
        #expect(PairingEntry.link(from: text) == nil)
    }

    @Test("a typed address gets pocketd's port when it has none", arguments: [
        ("100.64.0.1", "100.64.0.1:4517"), (" mac.tail1234.ts.net ", "mac.tail1234.ts.net:4517"), ("100.64.0.1:4518", "100.64.0.1:4518"),
        ("fd7a:115c:a1e0::1", "[fd7a:115c:a1e0::1]:4517"), ("[fd7a:115c:a1e0::1]:4600", "[fd7a:115c:a1e0::1]:4600"),
    ])
    func address(_ typed: String, _ want: String) {
        #expect(PairingEntry.address(typed) == want)
    }

    @Test("a typed address or code that can't be right is refused")
    func typedRefused() {
        #expect(PairingEntry.address("") == nil)
        #expect(PairingEntry.address("my mac") == nil)
        #expect(PairingEntry.link(host: "100.64.0.1", code: "short") == nil)
        #expect(PairingEntry.link(host: "100.64.0.1", code: " \(code) ") == PairingLink(host: "100.64.0.1:4517", code: code, name: "100.64.0.1:4517"))
    }
}

@Suite("Pairing flow")
@MainActor
struct PairingModelTests {
    @Test("a pair link opens straight at the confirm screen, and a revoked Mac's host at the code entry")
    func starts() {
        let link = URL(string: "anywhere://pair?v=1&h=127.0.0.1%3A4517&c=\(code)&n=Studio")!
        #expect(PairingModel(app: AppModel(), link: link, phoneName: "p").path == [.confirm(PairingLink(host: "127.0.0.1:4517", code: code, name: "Studio"))])
        #expect(PairingModel(app: AppModel(), knownHost: "100.64.0.1:4517", phoneName: "p").path == [.enter])
        #expect(PairingModel(app: AppModel(), phoneName: "p").path.isEmpty)
    }

    @Test("scanning a second code replaces the confirm screen instead of stacking another")
    func rescans() {
        let model = PairingModel(app: AppModel(), phoneName: "p")
        model.path = [.scan]
        #expect(model.open("100.64.0.1:4517 \(code)"))
        #expect(!model.open("not a code"))
        #expect(model.open("100.64.0.2:4517 \(code)"))
        #expect(model.path == [.scan, .confirm(PairingLink(host: "100.64.0.2:4517", code: code, name: "100.64.0.2:4517"))])
    }

    @Test("a Mac that can't be reached fails the attempt and adds no Mac")
    func unreachable() async {
        let app = AppModel()
        let model = PairingModel(app: app, phoneName: "p")
        await model.pair(PairingLink(host: "127.0.0.1:9", code: code, name: "Nowhere"))
        #expect(model.phase == .failed(.unreachable))
        #expect(app.hosts.isEmpty)
    }
}

@Suite("Pairing failures")
struct PairingFailureCopyTests {
    @Test("only an unreachable Mac points at Tailscale")
    func tailscale() {
        let all: [PairingFailure] = [.expired, .used, .invalid, .locked, .rateLimited, .unreachable, .updateMac, .updatePhone, .failed("x")]
        #expect(all.filter { $0.copy(mac: "Studio").suggestsTailscale } == [.unreachable])
        #expect(PairingFailure.unreachable.copy(mac: "Studio").title == "Can't reach Studio")
    }

    @Test("a Mac whose link ended says why: removed, or which side to update")
    func ended() {
        #expect(EndedView.copy(.revoked, mac: "Studio").title == "Removed from Studio")
        #expect(EndedView.copy(.updateMac, mac: "Studio").title == "Update Anywhere on Studio")
        #expect(EndedView.copy(.updatePhone, mac: "Studio").title == "Update this app")
    }

    @Test("a failure the phone doesn't know shows what the Mac said")
    func passthrough() {
        #expect(PairingFailure.failed("Pairing failed. Make a new code on your Mac.").copy(mac: "M").message == "Pairing failed. Make a new code on your Mac.")
    }
}
