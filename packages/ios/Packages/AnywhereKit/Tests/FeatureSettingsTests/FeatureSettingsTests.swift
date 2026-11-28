import DesignSystem
import Foundation
import Stores
import Testing
import Wire
@testable import FeatureSettings

@Suite("Settings")
struct FeatureSettingsTests {
    @Test("each phone scope reads as what it lets this iPhone do, granted or not")
    func grants() {
        let grants = MacFacts.grants([.observe, .drive, .approve])
        #expect(grants.map(\.scope) == [.observe, .drive, .approve, .spawn])
        #expect(grants.map(\.allowed) == [true, true, true, false])
    }

    @Test("a Mac too old to send scopes is taken to grant every phone scope")
    func oldMac() {
        #expect(MacFacts.grants(nil).filter { !$0.allowed }.isEmpty)
    }

    @Test("a connected Mac's status carries its version; any other state stands alone")
    func status() {
        #expect(MacFacts.status(.online, version: "0.1.13") == "Connected · 0.1.13")
        #expect(MacFacts.status(.online, version: nil) == "Connected")
        #expect(MacFacts.status(.reconnecting, version: "0.1.13") == "Reconnecting…")
        #expect(MacFacts.status(.unreachable, version: "0.1.13") == "Couldn't reconnect · Check Tailscale")
        #expect(MacFacts.status(.stopped, version: nil) == "Offline · Check Tailscale")
        #expect(MacFacts.status(.revoked, version: "0.1.13") == "Access removed")
    }

    @Test("a Mac that is gone says when it was last seen, in the reader's time zone")
    func lastSeen() {
        let utc = TimeZone(identifier: "UTC")!
        let at = 1_792_832_400_000 + 105 * 60_000
        #expect(MacFacts.detailStatus(.stopped, lastSeen: at, timeZone: utc) == "Offline · last seen 10:45")
        #expect(MacFacts.detailStatus(.unreachable, lastSeen: nil, timeZone: utc) == "Offline")
        #expect(MacFacts.detailStatus(.online, lastSeen: at, timeZone: utc) == "Connected")
    }

    @Test("the dot is green when up, red when the Mac can't be used, and grey otherwise")
    func dots() {
        #expect(MacFacts.dot(.online) == Palette.success)
        #expect(MacFacts.dot(.unreachable) == Palette.failed)
        #expect(MacFacts.dot(.revoked) == Palette.failed)
        #expect(MacFacts.dot(.reconnecting) == Palette.text3)
        #expect(MacFacts.dot(.stopped) == Palette.text4)
        #expect(MacFacts.dot(.updateMac) == Palette.text4)
    }

    @Test("what's working and what needs you read together, and leave out what is none")
    func activity() {
        #expect(MacFacts.activity(working: 2, needsYou: 1) == "2 working · 1 needs you")
        #expect(MacFacts.activity(working: 0, needsYou: 3) == "3 needs you")
        #expect(MacFacts.activity(working: 1, needsYou: 0) == "1 working")
        #expect(MacFacts.activity(working: 0, needsYou: 0) == nil)
    }

    @Test("a Mac named like a desktop gets the monitor, any other the laptop")
    func icons() {
        #expect(MacFacts.icon(forName: "Studio Mac Mini") == .monitor)
        #expect(MacFacts.icon(forName: "Minh's iMac") == .monitor)
        #expect(MacFacts.icon(forName: "Minh's MacBook Pro") == .laptop)
    }

    @Test("a Mac's own screen takes the layout its link calls for")
    func standing() {
        #expect(MacFacts.standing(.online) == .online)
        #expect([Reachability.connecting, .reconnecting, .unreachable, .stopped].allSatisfy { MacFacts.standing($0) == .offline })
        #expect(MacFacts.standing(.updateMac) == .versions && MacFacts.standing(.updatePhone) == .versions)
        #expect(MacFacts.standing(.revoked) == .removed)
    }

    @Test("only a link that ended for good has a message and a way back")
    func endedMessage() {
        #expect(MacFacts.endedMessage(.online, mac: "Studio", version: "0.1.40") == nil)
        #expect(MacFacts.endedMessage(.unreachable, mac: "Studio", version: nil) == nil)
        #expect(MacFacts.endedMessage(.revoked, mac: "Studio", version: nil)?.hasPrefix("Studio removed this iPhone.") == true)
        #expect(MacFacts.endedMessage(.updateMac, mac: "Studio", version: "0.1.40")?.contains("Zoes on the Mac is 0.1.40, older") == true)
        #expect(MacFacts.endedMessage(.updateMac, mac: "Studio", version: nil)?.contains("Zoes on the Mac is older") == true)
        #expect(MacFacts.endedMessage(.updatePhone, mac: "Studio", version: nil)?.hasPrefix("Studio runs a newer Zoes") == true)
    }

    @Test("a Mac's alerts read in a word, with the reason under it when they aren't simply on")
    func alerts() {
        #expect(MacFacts.alerts(.registered, permission: .granted) == "On")
        #expect(MacFacts.alerts(.waiting, permission: .denied) == "Off")
        #expect(MacFacts.alerts(.waiting, permission: .undetermined) == "Waiting")
        #expect(MacFacts.alerts(.failed("Nope."), permission: .granted) == "Failed")
        #expect(MacFacts.alertsNote(.registered, permission: .granted) == nil)
        #expect(MacFacts.alertsNote(.failed("Nope."), permission: .granted) == "Nope.")
        #expect(MacFacts.alertsNote(.waiting, permission: .denied) == "Notifications are off for Zoes on this iPhone.")
        #expect(MacFacts.alertsNote(.unsupported, permission: .granted) == "Update Zoes on this Mac to get notifications.")
    }

    @Test("the appearance picks a colour scheme, or none to follow the system")
    func appearance() {
        #expect(Appearance.system.colorScheme == nil)
        #expect(Appearance.dark.colorScheme == .dark)
        #expect(Appearance(rawValue: "light")?.colorScheme == .light)
    }

    @Test("the version shows alone in the list and with its build in About")
    func version() {
        let info: [String: Any] = ["CFBundleShortVersionString": "0.1.48", "CFBundleVersion": "212"]
        #expect(AppVersion.short(info) == "0.1.48")
        #expect(AppVersion.full(info) == "0.1.48 (212)")
        #expect(AppVersion.full(["CFBundleShortVersionString": "0.1.48"]) == "0.1.48")
        #expect(AppVersion.short(nil) == "Development")
    }
}
