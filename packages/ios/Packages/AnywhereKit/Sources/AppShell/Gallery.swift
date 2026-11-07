import DesignSystem
import Domain
import FeatureSettings
import Fixtures
import Foundation
import Stores
import SwiftUI

public enum AppShellGallery {
    static let phone = CGSize(width: 393, height: 852)
    static let pad = CGSize(width: 1180, height: 820)

    @MainActor public static var scenes: [GalleryScene] {
        [false, true].flatMap { dark in
            let suffix = dark ? "-dark" : ""
            return [
                GalleryScene("root-phone\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .sessions)) },
                GalleryScene("root-phone-terminals\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .terminals)) },
                GalleryScene("root-phone-settings\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .settings)) },
                GalleryScene("root-phone-reconnecting\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .sessions, mac: 1)) },
                GalleryScene("root-phone-revoked\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .sessions, mac: 2)) },
                GalleryScene("root-phone-unpaired\(suffix)", size: phone, dark: dark) { AnyView(RootView(app: AppModel(), layout: .phone)) },
                GalleryScene("root-phone-held\(suffix)", size: phone, dark: dark) {
                    AnyView(root(.phone, tab: .sessions, mac: 1) { navigator, app in
                        navigator.open(.agent(mac: app.hosts[1].mac, agentId: "a1"), macs: [MacRef(id: app.hosts[1].id, serverId: app.hosts[1].mac, online: false)])
                    })
                },
                GalleryScene("root-phone-ended-pushed\(suffix)", size: phone, dark: dark) {
                    AnyView(root(.phone, tab: .sessions, mac: 2) { navigator, app in
                        navigator.sessionsPath = [.project(mac: app.hosts[2].mac, path: "/Users/mingo/code/anywhere")]
                    })
                },
                GalleryScene("root-pad\(suffix)", size: pad, dark: dark) { AnyView(root(.pad, tab: .sessions)) },
                GalleryScene("root-pad-tree\(suffix)", size: pad, dark: dark) {
                    AnyView(root(.pad, tab: .sessions, tree: .worktree(mac: Sample.mac, path: "/Users/mingo/.worktrees/anywhere/fix-login")))
                },
                GalleryScene("root-pad-settings\(suffix)", size: pad, dark: dark) { AnyView(root(.pad, tab: .settings)) },
            ]
        }
    }

    /// The sample Macs from Settings' gallery, with the `mac`th one selected.
    @MainActor
    static func root(
        _ layout: RootView.Layout, tab: Tab, mac: Int = 0, tree: Route? = nil, then: (Navigator, AppModel) -> Void = { _, _ in }
    ) -> some View {
        let app = SettingsGallery.sampleApp()
        let navigator = Navigator()
        navigator.select(mac: app.hosts[mac].id)
        navigator.tab = tab
        navigator.tree = tree
        then(navigator, app)
        return RootView(app: app, layout: layout, navigator: navigator)
    }
}
