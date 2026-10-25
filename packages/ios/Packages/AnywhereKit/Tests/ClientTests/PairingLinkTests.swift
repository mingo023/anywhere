import Foundation
import Testing
@testable import Client

private let code = "q3xYq3xYq3xYq3xYq3xY_-"

@Suite("Pairing link")
struct PairingLinkTests {
    @Test("a pair link gives the host, code and Mac name")
    func parses() {
        let link = "anywhere://pair?v=1&h=100.77.122.82%3A4517&c=\(code)&n=Mingo%27s+MacBook+Pro"
        #expect(PairingLink.parse(link) == PairingLink(host: "100.77.122.82:4517", code: code, name: "Mingo's MacBook Pro"))
    }

    @Test("a pair link without a name is named after its host")
    func unnamed() {
        #expect(PairingLink.parse("anywhere://pair?v=1&h=127.0.0.1:4517&c=\(code)")?.name == "127.0.0.1:4517")
    }

    @Test("the link pocketd offers parses", arguments: ["anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv"])
    func offered(_ link: String) {
        #expect(PairingLink.parse(link)?.host == "100.64.0.1:4517")
    }

    @Test("pair links from another version or with a bad code or host are refused", arguments: [
        "anywhere://pair?v=2&h=127.0.0.1:4517&c=\(code)",
        "anywhere://pair?v=1&h=127.0.0.1:4517&c=short",
        "anywhere://pair?v=1&h=127.0.0.1&c=\(code)",
        "anywhere://pair?v=1&h=127.0.0.1:99999&c=\(code)",
        "anywhere://pair?v=1&h=%E0%A4%A&c=\(code)",
        "https://example.com/pair?v=1&h=127.0.0.1:4517&c=\(code)",
    ])
    func refuses(_ link: String) {
        #expect(PairingLink.parse(link) == nil)
    }

    @Test("a typed code must be the 22 characters from the Mac")
    func codes() {
        #expect(PairingLink.isCode(code))
        #expect(!PairingLink.isCode(String(code.dropFirst())))
        #expect(!PairingLink.isCode(String(code.dropFirst()) + "!"))
    }

    @Test("a Mac address needs a port")
    func hosts() {
        #expect(PairingLink.isHost("mac-mini.tail1234.ts.net:4517"))
        #expect(PairingLink.isHost("[fd7a:115c:a1e0::1]:4517"))
        #expect(!PairingLink.isHost("mac-mini.tail1234.ts.net"))
        #expect(!PairingLink.isHost("ws://127.0.0.1:4517"))
        #expect(!PairingLink.isHost(":4517"))
    }
}

@Suite("Daemon endpoint")
struct DaemonEndpointTests {
    @Test("loopback and tailnet hosts use plain ws, others wss")
    func schemes() {
        let cases = [
            ("127.0.0.1:4517", "ws://127.0.0.1:4517"), ("100.77.122.82:4517", "ws://100.77.122.82:4517"),
            ("[fd7a:115c:a1e0::1]:4517", "ws://[fd7a:115c:a1e0::1]:4517"),
            ("mac-mini.tail1234.ts.net:4517", "ws://mac-mini.tail1234.ts.net:4517"), ("localhost:4517", "ws://localhost:4517"),
            ("100.128.0.1:4517", "wss://100.128.0.1:4517"), ("example.com:443", "wss://example.com:443"),
            ("127.evil.example:1", "wss://127.evil.example:1"), ("100.64.evil.example:1", "wss://100.64.evil.example:1"),
        ]
        for (host, url) in cases {
            #expect(DaemonEndpoint.url(host: host)?.absoluteString == url, "\(host)")
        }
    }

    @Test("an address with a leading-zero octet is not the tailnet")
    func leadingZeros() {
        #expect(DaemonEndpoint.url(host: "100.064.0.1:4517")?.scheme == "wss")
        #expect(DaemonEndpoint.url(host: "127.0.0.01:4517")?.scheme == "wss")
        #expect(DaemonEndpoint.url(host: "100.64.0.0:4517")?.scheme == "ws")
    }
}
