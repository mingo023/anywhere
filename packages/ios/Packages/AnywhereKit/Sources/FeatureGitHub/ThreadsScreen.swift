import Domain
import DesignSystem
import MarkdownView
import Stores
import SwiftUI
import Wire

/// A PR's review threads by file: reply, resolve and reopen, with Undo.
@MainActor
struct ThreadsScreen: View {
    let store: GitHubStore
    let model: TreePullRequest
    var now: Int?

    @State private var filter = ThreadFilter()
    @State private var replying: ReviewThread?

    init(store: GitHubStore, model: TreePullRequest, now: Int? = nil, filter: ThreadFilter = ThreadFilter()) {
        self.store = store
        self.model = model
        self.now = now
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
            if groups.isEmpty {
                Section {
                    Label("All conversations resolved", systemImage: "checkmark.circle.fill").foregroundStyle(Palette.successText.color)
                }
            }
            ForEach(groups) { group in
                Section {
                    ForEach(group.threads) { thread in
                        ThreadRow(thread: thread, now: now ?? Int(Date().timeIntervalSince1970)) { replying = thread } resolve: { resolved in
                            Haptics.tap()
                            Task { await store.resolve(model.tree, thread: thread.id, resolved: resolved) }
                        }
                    }
                } header: {
                    Text(group.path).font(Typography.codeSmall).textCase(nil).lineLimit(1).truncationMode(.head)
                }
            }
        }
        .groupedList()
        .navigationTitle("Comments")
        .animation(.default, value: filter)
        .overlay(alignment: .bottom) {
            if let notice = model.notice {
                NoticeBar(notice: notice) { Task { await store.undo(model.tree) } }
            }
        }
        .animation(.snappy, value: model.notice)
        .sheet(item: $replying) { thread in
            ReplySheet(thread: thread) { body in
                Task { await store.reply(model.tree, thread: thread.id, body: body) }
            }
        }
    }
}

/// One thread: where it is, the code it's on, what was said, and what can be done about it.
@MainActor
struct ThreadRow: View {
    let thread: ReviewThread
    let now: Int
    let reply: () -> Void
    let resolve: (Bool) -> Void
    @Environment(\.openURL) private var openURL

    var body: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            HStack(spacing: Space.s) {
                Text(ThreadGroups.place(thread)).font(Typography.codeSmall).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                Spacer(minLength: 0)
                if thread.outdated { PrPill(text: "Outdated", tone: .quiet) }
                switch ThreadState(thread) {
                case .resolved: PrPill(text: "Resolved", tone: .success)
                case .replied: PrPill(text: "Replied", tone: .quiet)
                case .open: EmptyView()
                }
            }
            let hunk = ThreadGroups.hunkTail(thread.comments.first?.hunk)
            if !hunk.isEmpty && !thread.resolved { HunkBox(lines: hunk) }
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
                HStack(spacing: Space.m) {
                    Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
                    Button("Resolve", systemImage: "checkmark") { resolve(true) }
                }
                .buttonStyle(.bordered)
                .controlSize(.small)
                .labelStyle(.titleAndIcon)
            }
        }
        .padding(.vertical, Space.xs)
        .opacity(thread.resolved ? 0.7 : 1)
        .swipeActions(edge: .trailing) {
            if thread.resolved {
                Button("Unresolve", systemImage: "arrow.uturn.backward") { resolve(false) }.tint(.orange)
            } else {
                Button("Resolve", systemImage: "checkmark") { resolve(true) }.tint(.green)
            }
        }
        .swipeActions(edge: .leading) {
            Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply).tint(.blue)
        }
        .contextMenu {
            Button("Reply", systemImage: "arrowshape.turn.up.left", action: reply)
            if thread.resolved {
                Button("Unresolve", systemImage: "arrow.uturn.backward") { resolve(false) }
            } else {
                Button("Resolve", systemImage: "checkmark") { resolve(true) }
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

/// The reply composer, posted to GitHub as the Mac's gh account.
@MainActor
struct ReplySheet: View {
    let thread: ReviewThread
    let send: (String) -> Void
    @State private var text = ""
    @Environment(\.dismiss) private var dismiss
    @FocusState private var focused: Bool

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(ThreadGroups.replyPrompt(thread), text: $text, axis: .vertical)
                        .lineLimit(3...12)
                        .focused($focused)
                } footer: {
                    Text("Posts to GitHub as you")
                }
            }
            .formStyle(.grouped)
            .navigationTitle(ThreadGroups.place(thread))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Reply") {
                        Haptics.tap()
                        send(text)
                        dismiss()
                    }
                    .disabled(text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }
            }
            .onAppear { focused = true }
        }
        .presentationDetents([.medium, .large])
    }
}
