import ChatModel
import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

/// Chat screens from the fixtures, light and dark: one per transcript scenario, then the composer's states.
public enum ChatGallery {
    static let agentId = "a-gallery"

    @MainActor public static var scenes: [GalleryScene] {
        let transcripts = Sample.chatScenarios.map { scenario in
            Entry(name: "chat-\(scenario.name)", make: { store(Sample.chat(scenario.name)) })
        }
        let composer: [Entry] = [
            Entry(name: "chat-composer-permission", make: { store(Sample.chat("permissions"), requests: [permission]) }),
            Entry(name: "chat-composer-question", make: { store(Sample.chat("every-tool-kind"), requests: [Sample.chatQuestion(agentId: agentId)]) }),
            Entry(name: "chat-composer-observe-only", make: { store(Sample.chat("permissions"), requests: [permission], scopes: [.observe]) }),
            Entry(name: "chat-composer-queue", make: {
                let s = store(Sample.chat("streaming-live"), status: .working)
                for text in ["Then add tests for the error counts", "And update the README"] {
                    s.draft.text = text
                    s.send(queue: true)
                }
                s.draft.text = "Also keep sub-agents as their own rows"
                return s
            }),
            Entry(name: "chat-composer-goal-btw", make: {
                let s = store(Sample.chat("long-markdown"), goal: Goal(objective: "every golden test passes", status: "active", tokensUsed: 16510, tokenBudget: 50000))
                s.ask(btw: "Which crate holds the fold?")
                _ = s.apply(.agentBtw(.init(agentId: agentId, answer: "The fold lives in crates/chat/src/timeline.rs.")))
                return s
            }),
            Entry(name: "chat-composer-slash", make: {
                let s = store(Sample.chat("empty"))
                s.draft.text = "/"
                return s
            }),
            Entry(name: "chat-open-every-tool-kind", make: { opened(store(Sample.chat("every-tool-kind"))) }),
            Entry(name: "chat-open-subagent-brief-report", make: { opened(store(Sample.chat("subagent-brief-report"))) }),
            Entry(name: "chat-open-errors", make: { opened(store(Sample.chat("errors"))) }),
            Entry(name: "chat-open-background-agents", make: { opened(store(Sample.chat("background-agents"))) }),
            Entry(name: "chat-composer-context-full", make: { store(Sample.chat("code-blocks"), tokens: 184_000, window: 200_000) }),
        ]
        return (transcripts + composer).flatMap { scene in
            [false, true].map { dark in
                GalleryScene(dark ? "\(scene.name)-dark" : scene.name, dark: dark) {
                    AnyView(NavigationStack { ChatScreen(store: scene.make(), machine: "Mingo's MacBook Pro", openFile: { _ in }, live: false) })
                }
            }
        }
    }

    struct Entry {
        let name: String
        let make: @MainActor () -> ChatStore
    }

    static var permission: PermissionRequest {
        var p = Sample.permission
        p.agentId = agentId
        return p
    }

    /// Every fold, phase and step opened, to show the work they hide.
    @MainActor
    static func opened(_ s: ChatStore) -> ChatStore {
        for id in s.session.ids where id.hasSuffix("#fold") || id.contains("#p") { s.toggle(id, false) }
        for case .phase(let turn, let blocks) in s.session.rows {
            let id = s.session.ids[s.session.rows.firstIndex(of: .phase(turn: turn, blocks: blocks))!]
            for b in blocks { s.toggle("\(id)/\(b)", false) }
        }
        return s
    }

    /// A store holding `items`, as a chat opened on them shows them; working when its last turn is open.
    @MainActor
    static func store(
        _ items: [TimelineItem], status: AgentStatus? = nil, requests: [PermissionRequest] = [], scopes: [Scope]? = nil, goal: Goal? = nil,
        tokens: Int = 48_000, window: Int = 1_000_000
    ) -> ChatStore {
        let open = fold(items).turns.last.map { $0.end == nil } ?? false
        let agent = AgentSummary(
            id: agentId, terminalId: "t-\(agentId)", title: "Port the commit graph lanes", cwd: "/Users/mingo/dev/anywhere", provider: "claude",
            model: "claude-opus-5-5", effort: "high", access: "edits", status: status ?? (open ? .working : .idle), attached: true, epoch: 1,
            maxSeq: items.last?.seq ?? 0, createdAt: Sample.now - 600_000, updatedAt: Sample.now, branch: "mobile/chat", tokensUsed: tokens,
            contextWindow: window, goal: goal)
        var hello = Sample.hello
        if let scopes { hello.scopes = scopes }
        let s = ChatStore(agentId: agentId, clock: { Sample.now })
        s.seed(agent, requests: requests, hello: hello)
        _ = s.apply(.agentTimeline(.init(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: agent.maxSeq)))
        s.flush()
        return s
    }
}
