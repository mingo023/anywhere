import DesignSystem
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

    @Test("the most a phone can approve is named as on the desktop")
    func access() {
        #expect(["ask", "edits", "auto"].map(MacFacts.access) == ["Ask", "Edits", "Auto"])
    }

    @Test("terminals read as the Mac sets them, or as set there when it is too old to say")
    func terminals() {
        #expect(MacFacts.terminals(true) == "On on the Mac")
        #expect(MacFacts.terminals(false) == "Off on the Mac")
        #expect(MacFacts.terminals(nil) == "Set on the Mac")
    }

    @Test("a connected Mac's status carries its version; any other state stands alone")
    func status() {
        #expect(MacFacts.status(.online, version: "0.1.13") == "Connected · 0.1.13")
        #expect(MacFacts.status(.online, version: nil) == "Connected")
        #expect(MacFacts.status(.reconnecting, version: "0.1.13") == "Reconnecting…")
    }

    @Test("the appearance picks a colour scheme, or none to follow the system")
    func appearance() {
        #expect(Appearance.system.colorScheme == nil)
        #expect(Appearance.dark.colorScheme == .dark)
        #expect(Appearance(rawValue: "light")?.colorScheme == .light)
    }

    @Test("a Mac's dot is green online, amber while redialing, red when it can't be used")
    func dots() {
        #expect(MacRow.dot(.online) == Palette.success)
        #expect(MacRow.dot(.reconnecting) == Palette.waitingDot)
        #expect(MacRow.dot(.revoked) == Palette.failed)
    }
}
