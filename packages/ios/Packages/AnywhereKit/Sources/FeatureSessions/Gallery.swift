import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum SessionsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let screens: [(String, @MainActor (HostModel) -> AnyView)] = [
            ("sessions-home", { AnyView(HomeView(host: $0, mac: Sample.mac)) }),
            ("sessions-up-next", { AnyView(UpNextList(host: $0)) }),
            ("sessions-tree", { AnyView(TreeView(host: $0, mac: Sample.mac, key: "/Users/mingo/.worktrees/anywhere/calm-otter")) }),
            ("sessions-new", { AnyView(NewSessionView(host: $0, mac: Sample.mac, project: nil, place: nil)) }),
            ("sessions-new-worktree", { AnyView(NewWorktreeView(host: $0, project: Sample.projects[0].path)) }),
            ("sessions-automations", { AnyView(AutomationsView(host: $0, mac: Sample.mac)) }),
            ("sessions-launch", { AnyView(launch($0, steps: ["prepare", "verify", "fetch"])) }),
            ("sessions-launch-asking", { AnyView(launch($0, steps: [])) }),
            ("sessions-branches-failed", { _ in
                AnyView(BranchPicker(title: "Base", branches: nil, error: "\(Sample.mac) didn't answer in time", retry: {}, allowDefault: true, selection: .constant(nil)))
            }),
        ]
        // An iPhone SE's sheet, where the launch has to scroll.
        let small = GalleryScene("sessions-launch-small", size: CGSize(width: 320, height: 568)) {
            AnyView(NavigationStack { launch(host(), steps: ["prepare", "verify", "fetch"]) }.screenBackground())
        }
        return screens.flatMap { name, screen in
            [false, true].map { dark in
                GalleryScene(dark ? "\(name)-dark" : name, dark: dark) {
                    AnyView(NavigationStack { screen(host()) }.screenBackground().preferredColorScheme(dark ? .dark : .light))
                }
            }
        } + [small]
    }

    /// A launch into a new worktree of the first project, `steps` into it.
    @MainActor static func launch(_ host: HostModel, steps: [String]) -> LaunchProgress {
        let project = Sample.projects[0].path
        var draft = NewSessionDraft(project: project, place: .newWorktree)
        draft.prompt = "Fix the build"
        host.workspace.add(draft.plan(taken: [], localId: "", localName: "", neverAsks: false), requestId: "launch", title: draft.prompt, place: nil)
        host.apply(steps.map { .message(.agentProgress(.init(requestId: "launch", step: $0))) })
        return LaunchProgress(host: host, mac: Sample.mac, requestId: "launch")
    }

    @MainActor static func host() -> HostModel {
        HostModel.offline(hello: Sample.hello, messages: Sample.snapshot + [
            .terminalList(.init(terminals: Sample.terminals)), .agentProviders(Sample.providers),
        ])
    }
}
