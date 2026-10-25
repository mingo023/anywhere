import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum SessionsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let screens: [(String, @MainActor (HostModel) -> AnyView)] = [
            ("sessions-home", { AnyView(HomeView(host: $0, mac: Sample.mac)) }),
            ("sessions-tree", { AnyView(TreeView(host: $0, mac: Sample.mac, key: "/Users/mingo/.worktrees/anywhere/calm-otter")) }),
            ("sessions-new", { AnyView(NewSessionView(host: $0, mac: Sample.mac, project: nil, place: nil)) }),
            ("sessions-new-worktree", { AnyView(NewWorktreeView(host: $0, project: Sample.projects[0].path)) }),
            ("sessions-automations", { AnyView(AutomationsView(host: $0, mac: Sample.mac)) }),
        ]
        return screens.flatMap { name, screen in
            [false, true].map { dark in
                GalleryScene(dark ? "\(name)-dark" : name, dark: dark) {
                    AnyView(NavigationStack { screen(host()) }.screenBackground().preferredColorScheme(dark ? .dark : .light))
                }
            }
        }
    }

    @MainActor static func host() -> HostModel {
        HostModel.offline(hello: Sample.hello, messages: Sample.snapshot + [
            .terminalList(.init(terminals: Sample.terminals)), .agentProviders(Sample.providers),
        ])
    }
}
