import Client
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

/// The redesign's Inbox, Sessions, project and worktree frames, built from the design's sample data: two Macs, and the
/// projects anywhere, garden-mobile and fox-api. Times are relative to now, so "2m" reads as 2m.
enum RedesignGallery {
    @MainActor static var scenes: [GalleryScene] {
        var scenes: [GalleryScene] = []
        func add(_ frame: String, size: CGSize = CGSize(width: 393, height: 852), dark: [Bool] = [true, false], _ screen: @escaping @MainActor () -> AnyView) {
            for isDark in dark {
                scenes.append(GalleryScene("\(frame)-\(isDark ? "dark" : "light")", size: size, dark: isDark) {
                    AnyView(NavigationStack { screen() }.screenBackground().preferredColorScheme(isDark ? .dark : .light))
                })
            }
        }
        add("05-inbox-needs-you") { AnyView(inbox(Macs.inbox())) }
        add("06-inbox-empty") { AnyView(inbox(Macs.inbox(empty: true))) }
        add("07-inbox-loading") { AnyView(inbox(Macs.inbox(loading: true))) }
        add("08-inbox-offline") { AnyView(inbox(Macs.inbox(mini: "Studio Mac Mini", miniGone: .offline(at: (10, 42))))) }
        add("08b-inbox-reconnecting") { AnyView(inbox(Macs.inbox(bookGone: .reconnecting))) }
        add("08c-inbox-back-online") { AnyView(inbox(Macs.inbox(), back: true)) }
        add("08d-inbox-reconnect-failed") { AnyView(inbox(Macs.inbox(bookGone: .unreachable(at: (9, 39))))) }
        add("09-inbox-answer-failed") { AnyView(inbox(Macs.inbox(), failedAnswers: ["a-check"])) }
        add("10-sessions") { AnyView(SessionsHome(hosts: Macs.sessions())) }
        add("11-project-anywhere") {
            let host = Macs.book(sessions: true, extras: true)
            return AnyView(HomeView(host: host, mac: host.mac, only: Macs.anywhere))
        }
        add("12-worktree-flaky-checkout") {
            let host = Macs.book(sessions: true, extras: true)
            return AnyView(TreeView(host: host, mac: host.mac, key: Macs.flaky))
        }
        add("12b-worktree-empty") {
            let host = Macs.book(sessions: false)
            return AnyView(TreeView(host: host, mac: host.mac, key: Macs.flaky))
        }
        add("12c-worktree-offline") {
            let host = Macs.mini(gone: .offline(at: (10, 42)), name: "Studio Mac Mini")
            let strip = NetStrip(.offline, NetStrip.offline(mac: host.name, asOf: host.staleAsOf(at: .now)))
            return AnyView(TreeView(host: host, mac: host.mac, key: Macs.invoiceCache).environment(\.topStrip, AnyView(strip)))
        }
        return scenes
    }

    @MainActor
    private static func inbox(_ hosts: [HostModel], back: Bool = false, failedAnswers: Set<String> = []) -> InboxView {
        InboxView(hosts: hosts, open: nil, back: back, failedAnswers: failedAnswers)
    }

    /// How a Mac's link stands.
    enum Gone {
        case reconnecting
        /// Dropped at a clock time today (or yesterday, if it is earlier than that now), for good.
        case offline(at: (Int, Int))
        /// Dropped at a clock time and not coming back by itself.
        case unreachable(at: (Int, Int))
    }

    @MainActor
    enum Macs {
        static let anywhere = "/Users/minh/dev/anywhere"
        static let flaky = "/Users/minh/.worktrees/anywhere/flaky-checkout"
        static let invoiceCache = "/Users/minh/.worktrees/fox-api/invoice-cache"

        static func hello(_ id: String, _ name: String) -> HelloOk {
            HelloOk(serverId: id, hostname: name, protocolVersion: 3, caps: Caps.phone, protocol: .phone, scopes: [.observe, .drive, .approve, .spawn],
                    host: HostState(tailnet: true, keepingAwake: true, terminals: true), version: "0.1.13")
        }

        static func ago(_ minutes: Double) -> Int { Int(Date().timeIntervalSince1970 * 1000 - minutes * 60_000 - 20_000) }

        static func agent(
            _ id: String, _ title: String, _ status: AgentStatus, failed: Bool? = nil, minutes: Double, cwd: String, project: String, branch: String,
            activity: String? = nil, error: String? = nil, pinned: Bool? = nil
        ) -> AgentSummary {
            AgentSummary(
                id: id, terminalId: "t-\(id)", title: title, cwd: cwd, provider: "claude", model: "claude-sonnet-5", status: status, failed: failed, error: error,
                attached: true, pinned: pinned, epoch: 1, maxSeq: 0, createdAt: ago(minutes + 10), updatedAt: ago(minutes), project: project, worktree: branch,
                branch: branch, activity: activity)
        }

        static func ask(_ id: String, _ agent: String, _ command: String) -> ServerMessage {
            .permissionRequest(PermissionAsk(request: PermissionRequest(
                requestId: id, agentId: agent, toolName: "Bash", detail: .shell(command: command, description: nil))))
        }

        /// A Mac that dropped, as `gone` says; one that never did is online.
        static func make(_ id: String, _ name: String, _ messages: [ServerMessage], gone: Gone? = nil) -> HostModel {
            let hello = hello(id, name)
            switch gone {
            case nil:
                return HostModel.offline(hello: hello, messages: messages)
            case .reconnecting?:
                return HostModel.offline(hello: hello, messages: messages, droppedFor: 10)
            case .unreachable(let clock)?:
                return HostModel.offline(hello: hello, messages: messages, droppedFor: since(clock))
            case .offline(let clock)?:
                let host = HostModel.offline(hello: hello, messages: messages, droppedFor: since(clock))
                host.apply([.closed(.stopped)])
                return host
            }
        }

        /// Seconds since the clock time `(h, m)` last passed.
        static func since(_ clock: (Int, Int)) -> TimeInterval {
            let calendar = Calendar.current
            let now = Date()
            var at = calendar.date(bySettingHour: clock.0, minute: clock.1, second: 0, of: now) ?? now
            if at > now { at = calendar.date(byAdding: .day, value: -1, to: at) ?? at }
            return now.timeIntervalSince(at)
        }

        /// Minh's MacBook Pro: anywhere, garden-mobile and fox-api.
        static func book(name: String = "Minh's MacBook Pro", sessions: Bool, extras: Bool = false, gone: Gone? = nil) -> HostModel {
            let root = "/Users/minh/.worktrees"
            let projects = [
                Project(path: anywhere, name: "anywhere", worktrees: [
                    Worktree(name: "anywhere", path: anywhere, branch: "main", isMain: true),
                    Worktree(name: "flaky-checkout", path: flaky, branch: "flaky-checkout", isMain: false),
                    Worktree(name: "dark-mode", path: "\(root)/anywhere/dark-mode", branch: "dark-mode", isMain: false),
                    Worktree(name: "react-19", path: "\(root)/anywhere/react-19", branch: "react-19", isMain: false),
                ]),
                Project(path: "/Users/minh/dev/garden-mobile", name: "garden-mobile", worktrees: [
                    Worktree(name: "garden-mobile", path: "/Users/minh/dev/garden-mobile", branch: "main", isMain: true),
                    Worktree(name: "resume-crash", path: "\(root)/garden-mobile/resume-crash", branch: "resume-crash", isMain: false),
                    Worktree(name: "offline-sync", path: "\(root)/garden-mobile/offline-sync", branch: "offline-sync", isMain: false),
                ]),
                Project(path: "/Users/minh/dev/fox-api", name: "fox-api", worktrees: [
                    Worktree(name: "fox-api", path: "/Users/minh/dev/fox-api", branch: "main", isMain: true),
                    Worktree(name: "upload-limit", path: "\(root)/fox-api/upload-limit", branch: "upload-limit", isMain: false),
                ]),
            ]
            var messages: [ServerMessage] = [.projectList(ProjectList(projects: projects)), .localList(LocalList(locals: [
                Local(id: "review", project: "/Users/minh/dev/fox-api", name: "Review"),
            ]))]
            if sessions {
                var agents = [
                    agent("a-dark", "Add dark mode to settings", .working, minutes: 52.0 / 60, cwd: "\(root)/anywhere/dark-mode", project: "anywhere", branch: "dark-mode",
                          activity: "Reading src/theme/tokens.ts", pinned: true),
                    agent("a-check", "Fix flaky checkout test", .needsYou, minutes: 2, cwd: flaky, project: "anywhere", branch: "flaky-checkout"),
                    agent("a-queue", "Fix queue crash", .needsYou, minutes: 6, cwd: "\(root)/garden-mobile/resume-crash", project: "garden-mobile", branch: "resume-crash"),
                    agent("a-rate", "Rate-limit the upload route", .done, minutes: 5, cwd: "\(root)/fox-api/upload-limit", project: "fox-api", branch: "upload-limit"),
                ]
                if extras {
                    agents += [
                        agent("a-find", "Find why checkout times out", .done, minutes: 60, cwd: flaky, project: "anywhere", branch: "flaky-checkout"),
                        agent("a-repro", "Reproduce the flake locally", .idle, minutes: 60 * 30, cwd: flaky, project: "anywhere", branch: "flaky-checkout"),
                        agent("a-main", "Explain the pairing flow", .idle, minutes: 60 * 5, cwd: anywhere, project: "anywhere", branch: "main"),
                        agent("a-scratch", "Try the new CLI", .idle, minutes: 60 * 40, cwd: "/Users/minh/scratch", project: "scratch", branch: "main"),
                    ]
                }
                messages += [.agentList(AgentList(agents: agents)), ask("r-check", "a-check", "pnpm test checkout --repeat 20"),
                             ask("r-queue", "a-queue", "pnpm test queue --runInBand")]
            } else {
                messages.append(.agentList(AgentList(agents: [])))
            }
            let host = make("minh-mbp", name, messages, gone: gone)
            for tree in [
                status(flaky, branch: "flaky-checkout", ahead: 3, files: 4, added: 12, removed: 3),
                status("\(root)/anywhere/dark-mode", branch: "dark-mode", ahead: 1, files: 2, added: 5, removed: 0),
                status("\(root)/anywhere/react-19", branch: "react-19", ahead: 14, files: 0, added: 0, removed: 0),
                status(anywhere, branch: "main", ahead: 0, files: 0, added: 0, removed: 0),
            ] { host.git.take(tree) }
            host.github.showList(anywhere, GhPrs(items: [
                PrItem(tree: flaky, number: 42, title: "Fix flaky checkout", state: "OPEN", draft: false, gate: .checksFailed, checks: Checks(passed: 3, failed: 1, pending: 0)),
                PrItem(tree: "\(root)/anywhere/react-19", number: 214, title: "React 19", state: "OPEN", draft: false, gate: .ready, checks: Checks(passed: 4, failed: 0, pending: 0)),
            ]))
            return host
        }

        /// Studio Mac Mini: fox-api, with the session that failed.
        static func mini(gone: Gone? = nil, name: String = "Mac Mini", empty: Bool = false) -> HostModel {
            let root = "/Users/minh/dev/fox-api"
            let projects = [Project(path: root, name: "fox-api", worktrees: [
                Worktree(name: "fox-api", path: root, branch: "main", isMain: true),
                Worktree(name: "invoice-cache", path: invoiceCache, branch: "invoice-cache", isMain: false),
                Worktree(name: "export-csv", path: "/Users/minh/.worktrees/fox-api/export-csv", branch: "export-csv", isMain: false),
            ])]
            let agents = empty ? [] : [agent("a-cache", "Cache invoice queries", .done, failed: true, minutes: 9, cwd: invoiceCache, project: "fox-api", branch: "invoice-cache",
                                             error: "3 tests didn't pass")]
            let host = make("studio-mini", name, [.projectList(ProjectList(projects: projects)), .agentList(AgentList(agents: agents))], gone: gone)
            host.git.take(status(invoiceCache, branch: "invoice-cache", ahead: 2, files: 6, added: 40, removed: 8))
            host.github.showList(root, GhPrs(items: []))
            return host
        }

        /// The Inbox's two Macs.
        static func inbox(mini name: String = "Mac Mini", empty: Bool = false, loading: Bool = false, miniGone: Gone? = nil, bookGone: Gone? = nil) -> [HostModel] {
            if loading {
                return [HostModel.offline(hello: hello("minh-mbp", "MacBook Pro"), messages: []), HostModel.offline(hello: hello("studio-mini", name), messages: [])]
            }
            return [book(name: "MacBook Pro", sessions: !empty, gone: bookGone), mini(gone: miniGone, name: name, empty: empty)]
        }

        /// The Sessions home's two Macs.
        static func sessions() -> [HostModel] {
            [book(sessions: true, extras: true), mini(gone: .unreachable(at: (10, 42)), name: "Studio Mac Mini")]
        }

        static func status(_ tree: String, branch: String, ahead: Int, files: Int, added: Int, removed: Int) -> GitStatus {
            GitStatus(tree: tree, rev: "r-\(branch)", branch: branch, base: "main", ahead: ahead, behind: 0,
                      totals: GitTotals(files: files, added: added, removed: removed, staged: 0), files: [], commits: [])
        }
    }
}
