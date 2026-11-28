import DesignSystem
import Fixtures
import Stores
import SwiftUI
import Wire

/// The frames for starting a session and for automations: the new session sheet, its steps, and the automations list.
public enum LaunchGallery {
    @MainActor public static var scenes: [GalleryScene] {
        let frames: [(name: String, view: @MainActor () -> AnyView)] = [
            ("17-new-session", { sheet(top: 64) { NavigationStack { NewSessionView(host: host(), mac: Sample.mac, project: anywhere, place: nil) } } }),
            ("17-new-session-images", {
                let images = LaunchImages(
                    files: nil, macName: "MacBook Pro",
                    list: [.init(id: "a", name: "login.png", path: "/att/login.png"), .init(id: "b", name: "trace.png")])
                return sheet(top: 64) {
                    NavigationStack {
                        NewSessionView(host: host(), mac: Sample.mac, project: anywhere, place: nil, prompt: "Match the login screen to this", images: images)
                    }
                }
            }),
            ("17b-new-session-offline", {
                let studio = host(name: "Studio Mac Mini", droppedFor: 600)
                return sheet(top: 64) {
                    NavigationStack {
                        NewSessionView(host: studio, mac: Sample.mac, project: foxApi, place: nil, prompt: "Cache invoice queries for five minutes")
                    }
                }
            }),
            ("18-launch-progress", { sheet(top: 332) { NavigationStack { launch(host(), steps: ["prepare", "verify", "fetch", "worktree"]) } } }),
            ("18-launch-progress-failed", {
                sheet(top: 160) {
                    NavigationStack { launch(host(), steps: ["prepare", "verify", "fetch", "worktree", "copy", "setup"], failed: true) }
                }
            }),
            ("39-automations", { AnyView(NavigationStack { AutomationList(lines: automationLines, mac: Sample.mac, setEnabled: { _, _ in }, runNow: { _ in }) }) }),
            ("39b-automations-empty", { AnyView(NavigationStack { AutomationList(lines: [], mac: Sample.mac, setEnabled: { _, _ in }, runNow: { _ in }) }) }),
        ]
        return frames.flatMap { name, view in
            [true, false].map { dark in
                GalleryScene("\(name)-\(dark ? "dark" : "light")", dark: dark) {
                    AnyView(view().preferredColorScheme(dark ? .dark : .light))
                }
            }
        }
    }

    private static let anywhere = "/Users/minh/dev/anywhere"
    private static let foxApi = "/Users/minh/dev/fox-api"

    private static func project(_ path: String) -> Project {
        let name = path.split(separator: "/").last.map(String.init) ?? path
        return Project(path: path, name: name, worktrees: [Worktree(name: name, path: path, branch: "main", isMain: true)])
    }

    @MainActor private static func host(name: String = "MacBook Pro", droppedFor: TimeInterval? = nil) -> HostModel {
        var hello = Sample.hello
        hello.hostname = name
        let projects = [anywhere, "/Users/minh/dev/garden-mobile", foxApi].map(project)
        return HostModel.offline(
            hello: hello, messages: [.projectList(ProjectList(projects: projects)), .agentProviders(Sample.providers)], droppedFor: droppedFor)
    }

    /// A launch into a new worktree of the first project, `steps` into it.
    @MainActor private static func launch(_ host: HostModel, steps: [String], failed: Bool = false) -> LaunchProgress {
        var draft = NewSessionDraft(project: anywhere, place: .newWorktree)
        draft.prompt = "Add dark mode to settings"
        draft.name = "dark-mode"
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

    /// The design's three automations: the third is on a Mac that is offline.
    @MainActor private static var automationLines: [AutomationText.Line] {
        let utc = TimeZone(secondsFromGMT: 0)!
        let now = Sample.now + 31 * 24 * 3_600_000 + 41 * 60_000
        let hour = 3_600_000
        let project = { (path: String) in AutomationText.project(of: path, in: [Self.project(anywhere), Self.project("/Users/minh/dev/garden-mobile"), Self.project(foxApi)]) }
        let rows: [(Automation, Run, String, Bool)] = [
            (Automation(id: "a1", name: "Nightly dependency check", prompt: "", provider: .claude, folder: anywhere, schedule: .days([0, 1, 2, 3, 4, 5, 6], time: "02:00"), enabled: true),
             Run(id: "r1", automationId: "a1", status: .succeeded, trigger: "schedule", startedAt: now - 6 * hour), "MacBook Pro", true),
            (Automation(id: "a2", name: "Triage new issues", prompt: "", provider: .claude, folder: "/Users/minh/dev/garden-mobile", schedule: .days([1, 2, 3, 4, 5], time: "09:00"), enabled: true),
             Run(id: "r2", automationId: "a2", status: .failed, trigger: "schedule", startedAt: now - 2 * hour), "MacBook Pro", true),
            (Automation(id: "a3", name: "Weekly changelog draft", prompt: "", provider: .claude, folder: foxApi, schedule: .days([1], time: "08:00"), enabled: false),
             Run(id: "r3", automationId: "a3", status: .succeeded, trigger: "schedule", startedAt: now - 7 * 24 * hour), "Mac Mini", false),
        ]
        return rows.map { automation, run, mac, reachable in
            AutomationText.line(automation, last: run, project: project(automation.folder), mac: mac, reachable: reachable, now: now, timeZone: utc)
        }
    }

    /// `content` as a sheet over a dimmed screen, its top `top` points down.
    @MainActor private static func sheet(top: CGFloat, @ViewBuilder _ content: () -> some View) -> AnyView {
        let shape = UnevenRoundedRectangle(topLeadingRadius: 22, topTrailingRadius: 22, style: .continuous)
        return AnyView(
            ZStack(alignment: .top) {
                Color.black.ignoresSafeArea()
                Palette.background.color.opacity(0.55)
                    .clipShape(UnevenRoundedRectangle(topLeadingRadius: 14, topTrailingRadius: 14, style: .continuous))
                    .padding(.horizontal, top < 100 ? 14 : 0)
                    .padding(.top, 52)
                    .frame(maxHeight: .infinity, alignment: .top)
                content()
                    .background(Palette.surface.color)
                    .clipShape(shape)
                    .overlay(alignment: .top) {
                        Capsule().fill(Palette.text4.color.opacity(0.7)).frame(width: 36, height: 5).padding(.top, 6)
                    }
                    .padding(.top, top)
            })
    }
}
