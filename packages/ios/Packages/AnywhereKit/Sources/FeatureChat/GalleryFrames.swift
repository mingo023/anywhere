import DesignSystem
import Fixtures
import Foundation
import Stores
import SwiftUI
import Wire

extension ChatGallery {
    /// The redesign's frames 13 to 16c: the flaky checkout test chat in each of its states, dark and light.
    @MainActor static var frames: [GalleryScene] {
        let ask = PermissionRequest(
            requestId: "r-checkout", agentId: agentId, toolName: "Bash", detail: .shell(command: "pnpm test checkout --repeat 20", description: nil),
            feedback: true)
        let ninethirtynine = Calendar.current.date(bySettingHour: 9, minute: 39, second: 0, of: .now).map { Int($0.timeIntervalSince1970 * 1000) }
        let states: [Frame] = [
            Frame(id: "13-chat-normal", status: .idle),
            Frame(id: "14-chat-permission", status: .needsYou, requests: [ask]),
            Frame(id: "15-chat-working", status: .working),
            Frame(id: "16-chat-reconnecting", status: .idle, net: .reconnecting),
            Frame(id: "16b-chat-back-online", status: .idle, net: .back),
            Frame(id: "16c-chat-reconnect-failed", status: .idle, net: .failed(asOf: ninethirtynine)),
        ]
        return states.flatMap { frame in
            [true, false].map { dark in
                GalleryScene("\(frame.id)-\(dark ? "dark" : "light")", dark: dark) {
                    let host = HostModel.offline(hello: Sample.hello, messages: [])
                    host.git.take(branchStatus)
                    let chat = store(
                        checkoutItems(settled: frame.settled), status: frame.status, requests: frame.requests, access: "ask",
                        model: "claude-sonnet-4-5", title: "Fix flaky checkout test", branch: "flaky-checkout")
                    return AnyView(NavigationStack {
                        ChatScreen(
                            store: chat, machine: "MacBook Pro", host: host, openLink: { _ in }, live: false,
                            git: host.git.changes(cwd), menu: [ChatMenuItem(title: "Changes", icon: .fileDiff) {}], net: frame.net)
                    })
                }
            }
        }
    }

    /// The permission card on the narrowest screens, where its buttons share the least room.
    @MainActor static var permissionNarrow: [GalleryScene] {
        [true, false].map { dark in
            GalleryScene("14-chat-permission-narrow-\(dark ? "dark" : "light")", size: CGSize(width: 320, height: 568), dark: dark) {
                let ask = PermissionRequest(
                    requestId: "r-checkout", agentId: agentId, toolName: "Bash", detail: .shell(command: "pnpm test checkout --repeat 20", description: nil),
                    feedback: true)
                let chat = store(checkoutItems(settled: false), status: .needsYou, requests: [ask], branch: "flaky-checkout")
                return AnyView(NavigationStack {
                    ChatScreen(store: chat, machine: "MacBook Pro", host: HostModel.offline(hello: Sample.hello, messages: []), openLink: { _ in }, live: false)
                })
            }
        }
    }

    private struct Frame {
        let id: String
        let status: AgentStatus
        var requests: [PermissionRequest] = []
        var net: ChatNet?

        /// The turn is over, its work shown open; else it is under way, and the agent works or waits on an answer.
        var settled: Bool { requests.isEmpty && status != .working }
    }

    private static var branchStatus: GitStatus {
        var status = Sample.gitStatus(files: 1, tree: cwd)
        status.branch = "flaky-checkout"
        status.totals = GitTotals(files: 1, added: 12, removed: 3, staged: 0)
        return status
    }

    /// The design's transcript, begun 12 s ago by the wall clock, which the live line counts from.
    private static func checkoutItems(settled: Bool) -> [TimelineItem] {
        let start = Int(Date().timeIntervalSince1970 * 1000) - 12_000
        var seq = 0
        func item(_ body: TimelineBody) -> TimelineItem {
            seq += 1
            return TimelineItem(id: "c\(seq)", seq: seq, ts: start + seq * 100, body: body)
        }
        let diff = FileDiff(
            lines: [
                DiffLine(number: 14, kind: .del, text: "  const server = setupServer(...handlers)"),
                DiffLine(number: 14, kind: .add, text: "  beforeEach(async () => { await server.listen() })"),
            ], additions: 12, deletions: 3)
        var items = [
            item(.user(text: "The checkout test fails about one run in five on CI. Find out why and fix it.")),
            item(.tool(ToolCall(toolUseId: "read", name: "Read", detail: .read(path: "src/checkout/checkout.test.ts"), status: .ok, durationMs: 12))),
            item(.tool(ToolCall(toolUseId: "find", name: "Grep", detail: .search(query: "waitFor", path: nil), status: .ok, durationMs: 40))),
            item(.tool(ToolCall(toolUseId: "run", name: "Bash", detail: .shell(command: "pnpm test checkout", description: nil), status: .ok, durationMs: 8_400))),
            item(.assistant(text: """
                The test races the payment mock. `submitOrder` resolves before the mock registers its handler, so the cart is sometimes empty when the assertion runs.

                I moved the mock setup into `beforeEach` and now await `server.listen()`. Running it 20 times to confirm.
                """)),
            item(.tool(ToolCall(toolUseId: "edit", name: "Edit", detail: .edit(path: "checkout.test.ts", diff: diff), status: .ok, durationMs: 6))),
        ]
        if settled {
            items.append(item(.result(ok: true, durationMs: 11_000, error: nil, usage: TurnUsage(inputTokens: 12_000, outputTokens: 1_800, cacheReadTokens: 9_000))))
        }
        return items
    }
}

extension ChatGallery {
    /// The component sheet's permission card (frame 42): the composer of frame 14, which the card replaces, with its footer.
    @MainActor public static var sheetPermission: some View { SheetComposer() }

    private struct SheetComposer: View {
        @State private var store = ChatGallery.store(
            [], status: .needsYou, requests: [PermissionRequest(
                requestId: "r-checkout", agentId: agentId, toolName: "Bash", detail: .shell(command: "pnpm test checkout --repeat 20", description: nil),
                feedback: true)], branch: "flaky-checkout")
        @FocusState private var focus: ChatField?

        var body: some View {
            ComposerView(
                store: store, machine: "MacBook Pro", height: 852, outsideTaps: 0, show: { _ in }, mapOpen: .constant(false), openLink: { _ in },
                focus: $focus)
        }
    }
}
