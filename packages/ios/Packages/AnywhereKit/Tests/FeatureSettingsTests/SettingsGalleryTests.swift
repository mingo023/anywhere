import Testing
@testable import FeatureSettings

@Suite("Settings gallery")
struct SettingsGalleryTests {
    @MainActor
    @Test("every frame of Machines and its screens is a scene in light and dark")
    func frames() {
        let names = (SettingsGallery.scenes + NotificationsGallery.scenes).map(\.name)
        for frame in [
            "33-machines", "33b-machines-reconnecting", "33c-machines-back-online", "33d-machines-reconnect-failed", "34-machine-connected",
            "35-machine-offline", "35b-machine-version-mismatch", "35c-machine-revoked", "36-settings-notifications", "37-settings-appearance",
            "38-settings-about",
        ] {
            #expect(names.contains("\(frame)-dark") && names.contains("\(frame)-light"), "\(frame)")
        }
    }

    @MainActor
    @Test("the sample app keeps three Macs in the order the shell's gallery reads them")
    func sampleApp() {
        #expect(SettingsGallery.sampleApp().hosts.map(\.name) == ["Minh's MacBook Pro", "Studio Mac Mini", "Old iMac"])
    }
}
