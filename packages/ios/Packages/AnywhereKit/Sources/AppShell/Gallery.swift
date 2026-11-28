import DesignSystem
import Domain
import FeatureChat
import FeatureGitHub
import FeatureSessions
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
                GalleryScene("root-phone-inbox\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .inbox)) },
                GalleryScene("root-phone-sessions\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .sessions)) },
                GalleryScene("root-phone-prs\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .prs)) },
                GalleryScene("root-phone-machines\(suffix)", size: phone, dark: dark) { AnyView(root(.phone, tab: .machines)) },
                GalleryScene("root-phone-unpaired\(suffix)", size: phone, dark: dark) { AnyView(RootView(app: AppModel(), layout: .phone)) },
                GalleryScene("root-phone-held\(suffix)", size: phone, dark: dark) {
                    AnyView(root(.phone, tab: .sessions, mac: 1) { navigator, app in
                        navigator.open(.agent(mac: app.hosts[1].mac, agentId: "a1"), macs: [MacRef(id: app.hosts[1].id, serverId: app.hosts[1].mac, online: false)])
                    })
                },
                GalleryScene("root-phone-reconnecting-pushed\(suffix)", size: phone, dark: dark) {
                    AnyView(root(.phone, tab: .sessions, mac: 1) { navigator, app in
                        navigator.sessionsPath = [.project(mac: app.hosts[1].mac, path: "/Users/mingo/code/anywhere")]
                    })
                },
                GalleryScene("root-phone-ended-pushed\(suffix)", size: phone, dark: dark) {
                    AnyView(root(.phone, tab: .sessions, mac: 2) { navigator, app in
                        navigator.sessionsPath = [.project(mac: app.hosts[2].mac, path: "/Users/mingo/code/anywhere")]
                    })
                },
                GalleryScene("root-pad\(suffix)", size: pad, dark: dark) { AnyView(root(.pad, tab: .inbox)) },
                GalleryScene("root-pad-tree\(suffix)", size: pad, dark: dark) {
                    AnyView(root(.pad, tab: .sessions, tree: .worktree(mac: Sample.mac, path: "/Users/mingo/.worktrees/anywhere/fix-login")))
                },
                GalleryScene("root-pad-machines\(suffix)", size: pad, dark: dark) { AnyView(root(.pad, tab: .machines)) },
                GalleryScene("42-component-sheet-\(dark ? "dark" : "light")", size: DesignSystemGallery.sheetSize, dark: dark) {
                    AnyView(NavigationStack {
                        DesignSystemGallery.componentSheet(
                            sessions: AnyView(SessionsGallery.sheetRows), machines: AnyView(SettingsGallery.sheetMacRows),
                            prs: AnyView(GitHubGallery.sheetRows), permission: AnyView(ChatGallery.sheetPermission))
                    }
                    .screenBackground().preferredColorScheme(dark ? .dark : .light))
                },
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
