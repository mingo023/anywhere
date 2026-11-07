import ChatModel
import Domain
import DesignSystem
import MarkdownView
import Stores
import SwiftUI
import Wire

/// A PR's review threads by file: reply, resolve and reopen, with Undo, and send one or all open ones to the agent.
@MainActor
struct ThreadsScreen: View {
    let store: GitHubStore
    let model: TreePullRequest
    var mac = PrMac()
    var now: Int?
    var fix = FixContext()

    @State private var filter = ThreadFilter()
    @State private var replying: ReviewThread?
    @State private var fixing: FixRequest?

    init(
        store: GitHubStore, model: TreePullRequest, mac: PrMac = PrMac(), now: Int? = nil, filter: ThreadFilter = ThreadFilter(),
        fix: FixContext = FixContext()
    ) {
        self.store = store
        self.model = model
        self.mac = mac
        self.now = now
        self.fix = fix
        _filter = State(initialValue: filter)
    }

    var body: some View {
        let threads = model.pr?.threads ?? []
        let groups = ThreadGroups.group(threads, filter)
        List {
            Section {
                Picker("Show", selection: $filter.resolved) {
                    Text("Unresolved \(ThreadGroups.unresolved(threads))").tag(false)
                    Text("All \(threads.count)").tag(true)
                }
                .pickerStyle(.segmented)
                .labelsHidden()
                Toggle("Outdated", isOn: $filter.outdated)
            }
            if let empty = ThreadGroups.empty(threads, filter) {
                Section { emptyRow(empty) }
            }
            ForEach(groups) { group in
                Section {
                    ForEach(group.threads) { thread in
                        ThreadRow(
                            thread: thread, now: now ?? Int(Date().timeIntervalSince1970), file: file(thread), offline: mac.offline,
                            sent: model.fixing?.threads.contains(thread.id) ?? false, send: send(thread)
                        ) {
                            replying = thread
                        } resolve: { resolved in
                            Haptics.tap()
                            Task { await store.resolve(model.tree, thread: thread.id, resolved: resolved) }
                        }
                        if let body = model.unposted[thread.id] {
                            UnpostedReply(text: body, offline: mac.offline) {
                                Task { await store.reply(model.tree, thread: thread.id, body: body) }
                            } edit: {
                                store.unpost(model.tree, thread: thread.id, edit: true)
                                replying = thread
                            } delete: {
                                store.unpost(model.tree, thread: thread.id, edit: false)
                            }
                        }
                    }
                } header: {
                    Text(group.path).font(Typography.codeSmall).textCase(nil).lineLimit(1).truncationMode(.head)
                }
            }
            if let all = sendOpen {
                Section {
                    Button(action: all.action) { Text(all.title).frame(maxWidth: .infinity) }
                        .buttonStyle(.borderedProminent)
                        .disabled(mac.offline)
                }
            }
        }
        .groupedList()
        .navigationTitle("Comments")
        .errorAlert(model)
        .sendsFixes($fixing, store: store, tree: model.tree, context: fix)
        .animation(.default, value: filter)
        .overlay(alignment: .bottom) {
            if let notice = model.notice {
                NoticeBar(notice: notice) { Task { await store.undo(model.tree) } }.disabled(mac.offline)
            }
        }
        .animation(.snappy, value: model.notice)
        .sheet(item: $replying) { thread in
            ReplySheet(thread: thread, draft: model.drafts[thread.id] ?? "", footer: PullRequestText.postsAs(login: store.status?.login, mac: mac.name),
                       offline: mac.offline) {
                model.drafts[thread.id] = $0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : $0
            } send: { body in
                Task { await store.reply(model.tree, thread: thread.id, body: body) }
            }
        }
    }

    /// Send to the agent's title and action, nil when the thread can't be sent.
    private func send(_ thread: ReviewThread) -> (title: String, action: () -> Void)? {
        guard fix.canSend(fixing: model.fixing), let pr = model.pr else { return nil }
        return (fix.target.title("Send to"), { fixing = FixRequest(prompt: FixPrompt.fix(pr, checks: [], threads: [thread]), threads: [thread.id]) })
    }

    /// Send every open thread to the agent, offered while more than one is open.
    private var sendOpen: (title: String, action: () -> Void)? {
        guard fix.canSend(fixing: model.fixing), let pr = model.pr else { return nil }
        let open = ThreadGroups.unsent(pr, fixing: nil).threads
        guard open.count > 1 else { return nil }
        return (fix.target.title("Send \(open.count) open comments to"),
                { fixing = FixRequest(prompt: FixPrompt.fix(pr, checks: [], threads: open), threads: open.map(\.id)) })
    }

    /// The file a thread is on, at its line on the new side; the old side's line isn't in today's file.
    private func file(_ thread: ReviewThread) -> Route {
        .file(mac: mac.id, tree: model.tree, path: thread.path, line: thread.left == true ? nil : thread.line)
    }

    @ViewBuilder
    private func emptyRow(_ empty: ThreadGroups.Empty) -> some View {
        switch empty {
        case .noComments:
            Label("No review comments yet", systemImage: "text.bubble").foregroundStyle(Palette.text3.color)
        case .outdatedHidden(let n):
            HStack {
                Label("\(n) outdated \(n == 1 ? "conversation" : "conversations") hidden", systemImage: "eye.slash").foregroundStyle(Palette.text3.color)
                Spacer(minLength: 0)
                Button { filter.outdated = true } label: { Text("Show").hitTarget() }.buttonStyle(.borderless)
            }
        case .allResolved:
            Label("All conversations resolved", systemImage: "checkmark.circle.fill").foregroundStyle(Palette.successText.color)
        }
    }
}

/// One thread: where it is, the code it's on, what was said, and what can be done about it.
@MainActor
struct ThreadRow: View {
    let thread: ReviewThread
    let now: Int
    /// The code the thread is on, opened by tapping its place or hunk.
    let file: Route
    /// Resolving and sending wait for the Mac; replies can still be written.
    let offline: Bool
    /// Its agent still works on it.
    var sent = false
    /// Send to the agent, offered while the thread is open.
    var send: (title: String, action: () -> Void)?
    let reply: () -> Void
    let resolve: (Bool) -> Void
    @Environment(\.openURL) private var openURL

    private var state: ThreadState { ThreadState(thread, sent: sent) }

    private var sendable: (title: String, action: () -> Void)? { state == .open ? send : nil }

    @ViewBuilder
    private var sendButton: some View {
        if let sendable {
            Button(sendable.title, systemImage: "paperplane", action: sendable.action).buttonStyle(.borderedProminent).disabled(offline)
        }
    }

    @ViewBuilder
    private var replyAndResolve: some View {
        Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
        Button("Resolve", systemImage: "checkmark") { resolve(true) }.disabled(offline)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            NavigationLink(value: file) {
                VStack(alignment: .leading, spacing: Space.s) {
                    HStack(spacing: Space.s) {
                        Text(FixPrompt.place(thread)).font(Typography.codeSmall).foregroundStyle(Palette.accent.color).lineLimit(1)
                            .truncationMode(.head)
                        Spacer(minLength: 0)
                        if thread.outdated { PrPill(text: "Outdated", tone: .quiet) }
                        switch state {
                        case .resolved: PrPill(text: "Resolved", tone: .success)
                        case .sent: PrPill(text: "Fixing", tone: .waiting)
                        case .replied: PrPill(text: "Replied", tone: .quiet)
                        case .open: EmptyView()
                        }
                    }
                    .frame(minHeight: HitTarget.minimum)
                    let hunk = FixPrompt.hunkTail(thread.comments.first?.hunk)
                    if !hunk.isEmpty && !thread.resolved { HunkBox(lines: hunk) }
                }
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityHint("Opens the file")
            ForEach(Array((thread.resolved ? Array(thread.comments.prefix(1)) : thread.comments).enumerated()), id: \.offset) { _, c in
                VStack(alignment: .leading, spacing: Space.xxs) {
                    HStack(spacing: Space.xs) {
                        Text(c.author).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color)
                        Text(TimeText.when(ms: c.at * 1000, now: now * 1000)).font(Typography.caption).foregroundStyle(Palette.text4.color)
                    }
                    MarkdownView(c.body, style: .compact) { link in
                        if case .web(let url) = link { openURL(url) }
                    }
                    .lineLimit(thread.resolved ? 2 : nil)
                }
            }
            if !thread.resolved {
                // Send goes on its own line when the three don't fit, as the desktop's buttons wrap.
                ViewThatFits(in: .horizontal) {
                    HStack(spacing: Space.m) {
                        sendButton
                        replyAndResolve
                    }
                    VStack(alignment: .leading, spacing: Space.s) {
                        sendButton
                        HStack(spacing: Space.m) { replyAndResolve }
                    }
                }
                .buttonStyle(.bordered)
                .controlSize(.regular)
                .labelStyle(.titleAndIcon)
            }
        }
        .padding(.vertical, Space.xs)
        .opacity(thread.resolved ? 0.7 : 1)
        .swipeActions(edge: .trailing) {
            if thread.resolved {
                Button("Unresolve", systemImage: "arrow.uturn.backward") { resolve(false) }.tint(.orange).disabled(offline)
            } else {
                Button("Resolve", systemImage: "checkmark") { resolve(true) }.tint(.green).disabled(offline)
            }
        }
        .swipeActions(edge: .leading) {
            Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply).tint(.blue)
            if let sendable { Button(sendable.title, systemImage: "paperplane", action: sendable.action).tint(.indigo).disabled(offline) }
        }
        .contextMenu {
            if let sendable { Button(sendable.title, systemImage: "paperplane", action: sendable.action).disabled(offline) }
            Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
            if thread.resolved {
                Button("Unresolve", systemImage: "arrow.uturn.backward") { resolve(false) }.disabled(offline)
            } else {
                Button("Resolve", systemImage: "checkmark") { resolve(true) }.disabled(offline)
            }
        }
    }
}

/// The last lines of the hunk a thread is on, coloured as a diff.
@MainActor
struct HunkBox: View {
    let lines: [String]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(Array(lines.enumerated()), id: \.offset) { _, line in
                Text(line.isEmpty ? " " : line)
                    .font(Typography.codeSmall)
                    .foregroundStyle(Self.text(line).color)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, Space.s)
                    .background(Self.fill(line).color)
            }
        }
        .padding(.vertical, Space.xs)
        .background(Palette.markdownCodeBackground.color)
        .clipShape(RoundedRectangle(cornerRadius: Radius.small, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: Radius.small, style: .continuous).strokeBorder(Palette.markdownCodeBorder.color))
    }

    nonisolated static func text(_ line: String) -> Token {
        line.hasPrefix("+") ? Palette.diffAddText : line.hasPrefix("-") ? Palette.diffDelText : Palette.textBody
    }

    nonisolated static func fill(_ line: String) -> Token {
        line.hasPrefix("+") ? Palette.diffAddBackground : line.hasPrefix("-") ? Palette.diffDelBackground : Token.fixed(0)
    }
}

/// The reply composer, posted to GitHub as the Mac's gh account. What isn't sent stays as the thread's draft.
@MainActor
struct ReplySheet: View {
    let thread: ReviewThread
    let footer: String
    let offline: Bool
    let keep: (String) -> Void
    let send: (String) -> Void
    @State private var text: String
    @State private var sent = false
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focused: Bool

    init(
        thread: ReviewThread, draft: String = "", footer: String, offline: Bool = false, keep: @escaping (String) -> Void,
        send: @escaping (String) -> Void
    ) {
        self.thread = thread
        self.footer = footer
        self.offline = offline
        self.keep = keep
        self.send = send
        _text = State(initialValue: draft)
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(ThreadGroups.replyPrompt(thread), text: $text, axis: .vertical)
                        .lineLimit(3...12)
                        .focused($focused)
                } footer: {
                    Text(footer)
                }
            }
            .formStyle(.grouped)
            .navigationTitle(FixPrompt.place(thread))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Reply") {
                        Haptics.tap()
                        sent = true
                        send(text)
                        dismiss()
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || offline)
                }
            }
            .onAppear { focused = true }
            .onDisappear { if !sent { keep(text) } }
        }
        .presentationDetents([.medium, .large])
    }
}
