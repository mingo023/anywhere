import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

public enum SessionsGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let screens: [(String, @MainActor (HostModel) -> AnyView)] = [
            ("sessions-new", { AnyView(NewSessionView(host: $0, mac: Sample.mac, project: nil, place: nil)) }),
            ("sessions-new-images", { host in
                let images = LaunchImages(
                    files: nil, macName: Sample.mac,
                    list: [.init(id: "a", name: "login.png", path: "/att/login.png"), .init(id: "b", name: "trace.png")])
                return AnyView(NewSessionView(host: host, mac: Sample.mac, project: nil, place: nil, prompt: "Match the login screen to this", images: images))
            }),
            ("sessions-new-worktree", { AnyView(NewWorktreeView(host: $0, project: Sample.projects[0].path)) }),
            ("sessions-automations", { AnyView(AutomationsView(host: $0, mac: Sample.mac)) }),
            ("sessions-launch", { AnyView(launch($0, steps: ["prepare", "verify", "fetch"])) }),
            ("sessions-launch-asking", { AnyView(launch($0, steps: [])) }),
            ("sessions-launch-failed", { AnyView(launch($0, steps: ["prepare", "verify", "fetch", "worktree", "copy", "setup"], failed: true)) }),
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
        } + [small] + RedesignGallery.scenes
    }

    /// The component sheet's session rows (frame 42): one of each status, as the Inbox draws them, but the working one
    /// shows what it is doing, as its own list does.
    @MainActor public static var sheetRows: some View {
        let book = RedesignGallery.Macs.book(name: "MacBook Pro", sessions: true, extras: true)
        let mini = RedesignGallery.Macs.mini()
        func inbox(_ host: HostModel, _ id: String) -> some View {
            InboxRow(host: host, session: host.sessions.session(id)!, entry: InboxLayout.Entry(mac: host.id, id: id, dim: 1, locked: false), showMac: true)
        }
        return VStack(spacing: 0) {
            inbox(book, "a-check")
            SessionRow(session: book.sessions.session("a-dark")!).rowDivider()
            inbox(mini, "a-cache").rowDivider()
            inbox(book, "a-rate").rowDivider()
            inbox(book, "a-repro").rowDivider()
        }
        .insetGroup()
    }

    /// A launch into a new worktree of the first project, `steps` into it.
    @MainActor static func launch(_ host: HostModel, steps: [String], failed: Bool = false) -> LaunchProgress {
        let project = Sample.projects[0].path
        var draft = NewSessionDraft(project: project, place: .newWorktree)
        draft.prompt = "Fix the build"
        draft.name = "fix-the-build"
        draft.base = "main"
        host.workspace.add(draft.plan(taken: [], localId: "", localName: "", neverAsks: false), requestId: "launch", title: draft.prompt, place: nil)
        host.apply(steps.map { .message(.agentProgress(.init(requestId: "launch", step: $0))) })
        var shown = host.workspace.creations["launch"]!
        let took: [TimeInterval] = [0.4, 0.3, 2.2, 0.5, 0.1, 1.8]
        shown.started = Date().addingTimeInterval(-9)
        for i in shown.steps.indices { shown.steps[i].at = shown.started.addingTimeInterval(took.prefix(i).reduce(0, +)) }
        if failed { shown.error = ServerError(message: "Setup exited 1", detail: "npm ERR! missing script: setup") }
        return LaunchProgress(host: host, mac: Sample.mac, requestId: "launch", shown: shown)
    }

    @MainActor static func host() -> HostModel {
        HostModel.offline(hello: Sample.hello, messages: Sample.snapshot + [
            .terminalList(.init(terminals: Sample.terminals)), .agentProviders(Sample.providers),
        ])
    }
}
