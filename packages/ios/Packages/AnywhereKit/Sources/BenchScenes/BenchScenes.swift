import Client
import DesignSystem
import Domain
import FeatureChat
import FeatureFiles
import FeatureGit
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

/// The real screens over fixture stores, with the live updates to time: run by `anywhere-bench ui` in an offscreen
/// `NSHostingView` and, in a debug build, by the app (`-bench <name>`) on iOS.
@MainActor
public enum BenchScenes {
    public struct Scenario {
        public let name: String
        public var size = CGSize(width: 393, height: 852)
        /// Bottom-anchored: scrolling goes up.
        public var up = false
        /// Checked to open at its end and keep to it through the updates.
        public var follows = false
        public let make: @MainActor () -> Screen

        public init(name: String, size: CGSize = CGSize(width: 393, height: 852), up: Bool = false, follows: Bool = false,
                    make: @escaping @MainActor () -> Screen) {
            self.name = name
            self.size = size
            self.up = up
            self.follows = follows
            self.make = make
        }
    }

    public struct Screen {
        public let view: AnyView
        /// One live update, made ahead so building it isn't timed.
        public var updates: [@MainActor () -> Void] = []

        public init(view: AnyView, updates: [@MainActor () -> Void] = []) {
            self.view = view
            self.updates = updates
        }
    }

    public static let scenarios: [Scenario] = [
        Scenario(name: "chat-5000", up: true, follows: true, make: chat),
        Scenario(name: "changes-1500", make: changes),
        Scenario(name: "diff-20k", make: { diff(sideBySide: false) }),
        Scenario(name: "diff-20k-split", size: CGSize(width: 1024, height: 768), make: { diff(sideBySide: true) }),
        Scenario(name: "file-10k", make: file),
        Scenario(name: "markdown-258k", make: markdown),
    ]

    /// Run only by name: a prompt sent at the end of a long chat glides up and is held a quarter down (see its PNGs).
    public static var extras: [Scenario] {
        var extras = [
            Scenario(name: "chat-send", up: true, make: chatSend), Scenario(name: "chat-agents", up: true, make: { chatAgents() }),
            Scenario(name: "chat-subagent", make: { chatAgents(open: "tu-a1") }),
        ]
        #if os(iOS)
            // The sheet iOS selects markdown text in, over the 258 KB document.
            extras.append(Scenario(name: "select-text", make: { Screen(view: AnyView(SelectTextSheet(Sample.markdownDocument(bytes: 258 * 1024)))) }))
        #endif
        return extras
    }

    // MARK: Scenarios

    public static let agentId = "a-perf"

    public static func chat() -> Screen {
        var items = Sample.chatPerf(items: Int(ProcessInfo.processInfo.environment["UIBENCH_ITEMS"] ?? "") ?? 5000)
        let seq = items.count
        items.append(TimelineItem(id: "s-u", seq: seq + 1, ts: Sample.now, body: .user(text: "Stream a long answer")))
        items.append(TimelineItem(id: "s-a", seq: seq + 2, ts: Sample.now, body: .assistant(text: "Streaming ")))
        let agent = AgentSummary(
            id: agentId, terminalId: "t-perf", title: "Perf", cwd: "/Users/mingo/dev/anywhere", provider: "claude", model: "claude-opus-5-5",
            effort: "high", access: "edits", status: .working, attached: true, epoch: 1, maxSeq: seq + 2, createdAt: Sample.now - 600_000,
            updatedAt: Sample.now, branch: "perf", tokensUsed: 48_000, contextWindow: 1_000_000)
        let host = HostModel.offline(hello: Sample.hello, messages: [.agentList(.init(agents: [agent]))])
        let store = host.chat(agentId)
        host.apply([.message(.agentTimeline(.init(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: seq + 2)))])
        store.flush()
        var text = "Streaming "
        let frames = Int(ProcessInfo.processInfo.environment["UIBENCH_FRAMES"] ?? "") ?? 60
        let updates = (0..<frames).map { frame -> @MainActor () -> Void in
            text += frame % 20 == 19 ? "end.\n\n" : "word "
            let item = TimelineItem(id: "s-a", seq: seq + 2, ts: Sample.now, body: .assistant(text: text))
            return { host.apply([.message(.agentStream(.init(agentId: agentId, epoch: 1, item: item)))]) }
        }
        return Screen(view: AnyView(NavigationStack { ChatView(host: host, agentId: agentId) }), updates: updates)
    }

    public static func chatSend() -> Screen {
        let items = Sample.chatPerf(items: Int(ProcessInfo.processInfo.environment["UIBENCH_ITEMS"] ?? "") ?? 5000)
        let seq = items.count
        let agent = AgentSummary(
            id: agentId, terminalId: "t-perf", title: "Perf", cwd: "/Users/mingo/dev/anywhere", provider: "claude", model: "claude-opus-5-5",
            effort: "high", access: "edits", status: .idle, attached: true, epoch: 1, maxSeq: seq, createdAt: Sample.now - 600_000,
            updatedAt: Sample.now, branch: "perf", tokensUsed: 48_000, contextWindow: 1_000_000)
        let host = HostModel.offline(hello: Sample.hello, messages: [.agentList(.init(agents: [agent]))])
        let store = host.chat(agentId)
        host.apply([.message(.agentTimeline(.init(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: seq)))])
        store.flush()
        let prompt = "Hold this prompt a quarter down"
        let updates: [@MainActor () -> Void] = [{
            store.draft.text = prompt
            store.send()
        }]
        return Screen(view: AnyView(NavigationStack { ChatView(host: host, agentId: agentId) }), updates: updates)
    }

    /// A chat that spawned three sub-agents, still at work, under turns long enough to scroll beneath the bar; its
    /// updates are the first agent's next steps. Pushed over a list as the app does, with the sub-agent `open` spawned over it.
    public static func chatAgents(open: String? = nil) -> Screen {
        let shift = Int(Date.now.timeIntervalSince1970 * 1000) - Sample.now
        let earlier = Sample.chatPerf(items: 48)
        let agents = Sample.chat("background-agents").enumerated().map { n, item in
            TimelineItem(id: "ba-\(item.id)", seq: earlier.count + n + 1, ts: item.ts, parentToolUseId: item.parentToolUseId, body: item.body)
        }
        let items = (earlier + agents).map { TimelineItem(id: $0.id, seq: $0.seq, ts: $0.ts + shift, parentToolUseId: $0.parentToolUseId, body: $0.body) }
        let seq = items.count
        let agent = AgentSummary(
            id: agentId, terminalId: "t-perf", title: "Discover this project", cwd: "/Users/mingo/dev/anywhere", provider: "claude", model: "claude-opus-5-5",
            effort: "high", access: "edits", status: .working, attached: true, epoch: 1, maxSeq: seq, createdAt: Sample.now + shift - 600_000,
            updatedAt: Sample.now + shift, branch: "main", tokensUsed: 48_000, contextWindow: 1_000_000)
        let host = HostModel.offline(hello: Sample.hello, messages: [.agentList(.init(agents: [agent]))])
        let store = host.chat(agentId)
        host.apply([.message(.agentTimeline(.init(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: seq)))])
        store.flush()
        let steps = ["docs/adr/0003-desktop-code-layout.md", "docs/adr/0007-ios-modules.md", "CONTEXT.md"].enumerated().map { n, path in
            TimelineItem(
                id: "live-\(n)", seq: seq + n + 1, ts: Sample.now + shift, parentToolUseId: "tu-a1",
                body: .tool(.init(toolUseId: "tu-live-\(n)", name: "Read", detail: .read(path: path), status: .ok, durationMs: 10)))
        }
        let updates = steps.map { item -> @MainActor () -> Void in
            { host.apply([.message(.agentStream(.init(agentId: agentId, epoch: 1, item: item)))]) }
        }
        let chat = Route.chat(mac: Sample.mac, agentId: agentId)
        let path = [chat] + (open.map { [.subAgent(mac: Sample.mac, agentId: agentId, callId: $0)] } ?? [])
        return Screen(view: AnyView(BenchStack(host: host, path: path)), updates: updates)
    }

    public static func offlineHost() -> HostModel {
        HostModel(credentials: Credentials(host: "127.0.0.1:9", deviceToken: "t", deviceId: "bench", macName: "Mac", clientId: "bench"))
    }

    public static func changes() -> Screen {
        let host = offlineHost()
        let status = Sample.gitStatus(files: 1500)
        host.git.take(status)
        // A refresh after the Mac said the tree moved: a new rev, one file's counts changed.
        let updates = (0..<20).map { n -> @MainActor () -> Void in
            var next = status
            next.rev = "moved-\(n)"
            next.files[n * 37 % next.files.count].added = (next.files[n * 37 % next.files.count].added ?? 0) + n + 1
            return { host.git.take(next) }
        }
        return Screen(view: AnyView(NavigationStack { GitView(host: host, route: .changes(mac: Sample.mac, tree: Sample.tree)) }), updates: updates)
    }

    public static func diff(sideBySide: Bool) -> Screen {
        let host = offlineHost()
        let diff = Sample.diff(lines: 20_000)
        host.git.take(diff, at: .working)
        UserDefaults.standard.set(sideBySide, forKey: "git.diff.sideBySide")
        return Screen(view: AnyView(NavigationStack {
            GitView(host: host, route: .diff(mac: Sample.mac, tree: Sample.tree, path: diff.path, staged: false))
        }))
    }

    public static func preview(_ path: String, text: String, language: String) -> Screen {
        let host = offlineHost()
        _ = host.files.preview(tree: Sample.fileTree, path: path)
        _ = host.files.apply(.fsRead(FsRead(
            tree: Sample.fileTree, path: path, kind: .text, size: text.utf8.count, mtime: 1, text: text, language: language)))
        return Screen(view: AnyView(NavigationStack { FilesView(host: host, route: .file(mac: Sample.mac, tree: Sample.fileTree, path: path)) }))
    }

    public static func file() -> Screen {
        let lines = Sample.rustSource(bytes: 1 << 20).split(separator: "\n", omittingEmptySubsequences: false).prefix(10_000)
        UserDefaults.standard.set(false, forKey: "files.markdownSource")
        return preview("src/lanes.rs", text: lines.joined(separator: "\n"), language: "rust")
    }

    public static func markdown() -> Screen {
        UserDefaults.standard.set(false, forKey: "files.markdownSource")
        return preview("docs/long.md", text: Sample.markdownDocument(bytes: 258 * 1024), language: "markdown")
    }
}
