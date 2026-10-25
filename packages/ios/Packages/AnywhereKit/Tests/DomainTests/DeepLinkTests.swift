import Foundation
import Testing
@testable import Domain

@Suite("Deep links")
struct DeepLinkTests {
    @Test("a Mac link opens that Mac's sessions")
    func mac() {
        #expect(DeepLink.parse("anywhere://mac/mac-1") == .mac("mac-1"))
        #expect(DeepLink.parse("anywhere://mac/mac-1")?.routes == [.sessions(mac: "mac-1")])
    }

    @Test("an agent link opens its chat over its Mac's sessions")
    func agent() {
        let link = DeepLink.parse("anywhere://mac/Mingo%27s%20Mac/agent/a1")
        #expect(link == .agent(mac: "Mingo's Mac", agentId: "a1"))
        #expect(link?.routes == [.sessions(mac: "Mingo's Mac"), .chat(mac: "Mingo's Mac", agentId: "a1")])
    }

    @Test("a pair link goes to pairing with the whole link")
    func pair() {
        let link = "anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv"
        #expect(DeepLink.parse(link) == .pair(URL(string: link)!))
        #expect(DeepLink.parse(link)?.routes == [.pairing])
    }

    @Test("links to anything else are refused", arguments: [
        "https://mac/m1", "anywhere://mac", "anywhere://mac/m1/agent", "anywhere://mac/m1/terminal/t1", "anywhere://session/a1",
        "anywhere://mac/m1/agent/a1/more",
    ])
    func refuses(_ link: String) {
        #expect(DeepLink.parse(link) == nil)
    }

    @Test("a tapped notification opens its link, else the agent or Mac its Notice names")
    func notification() {
        #expect(DeepLink.notification(["link": "anywhere://mac/m1/agent/a1"]) == .agent(mac: "m1", agentId: "a1"))
        #expect(DeepLink.notification(["mac": "m1", "agent": "a2"]) == .agent(mac: "m1", agentId: "a2"))
        #expect(DeepLink.notification(["mac": "m1"]) == .mac("m1"))
        #expect(DeepLink.notification(["aps": ["alert": "x"]]) == nil)
    }

    @Test("a link written from its parts parses back to them")
    func roundTrip() {
        for link in [DeepLink.mac("mac 1"), .agent(mac: "m/1", agentId: "a?1")] {
            #expect(DeepLink.parse(link.url) == link)
        }
    }

    @Test("a route knows its Mac and survives encoding for state restoration")
    func routes() throws {
        let route = Route.diff(mac: "m", tree: "/w", path: "a.go", staged: true)
        #expect(route.mac == "m")
        #expect(Route.settings.mac == nil)
        #expect(try JSONDecoder().decode(Route.self, from: JSONEncoder().encode(route)) == route)
    }
}
