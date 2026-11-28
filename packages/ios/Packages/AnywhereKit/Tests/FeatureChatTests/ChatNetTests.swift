import Client
import Foundation
import Stores
import Testing
@testable import FeatureChat

@Suite("Chat network strip")
struct ChatNetTests {
    private func read(_ link: LinkState, offlineFor: TimeInterval, back: Bool = false) -> ChatNet? {
        ChatNet.read(Reachability(link: link, offlineFor: offlineFor), back: back, asOf: 1_000)
    }

    @Test("a blip shows nothing, then reconnecting, then a failed reconnect that says since when")
    func dropsEscalate() {
        #expect(read(.retrying(seconds: 1), offlineFor: 3) == nil)
        #expect(read(.retrying(seconds: 1), offlineFor: 5) == .reconnecting)
        #expect(read(.retrying(seconds: 8), offlineFor: 31) == .failed(asOf: 1_000))
    }

    @Test("back online shows only while it is asked for, and a Mac online otherwise shows nothing")
    func backOnline() {
        #expect(read(.online, offlineFor: 0, back: true) == .back)
        #expect(read(.online, offlineFor: 0) == nil)
    }

    @Test("a Mac that ended for good is the shell's to say")
    func endedLinksAreLeftToTheShell() {
        #expect(read(.ended(.revoked), offlineFor: 60) == nil)
        #expect(read(.ended(.updateMac), offlineFor: 60) == nil)
    }

    @Test("reconnecting and failed freeze answers and sends; back online doesn't")
    func freezing() {
        #expect(ChatNet.reconnecting.frozen)
        #expect(ChatNet.failed(asOf: nil).frozen)
        #expect(!ChatNet.back.frozen)
    }

    @Test("the permission card's buttons share its row 1 : 1 : 1.3, or 1 : 1.3 without Reply")
    func buttonShares() {
        #expect(PermissionCard.shares(feedback: true) == [1, 1, 1.3])
        #expect(PermissionCard.shares(feedback: false) == [1, 1.3])
    }
}
