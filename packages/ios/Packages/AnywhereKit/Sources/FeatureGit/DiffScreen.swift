import CodeView
import DesignSystem
import DiffModel
import Domain
import FileIcons
import Stores
import SwiftUI
import Wire

/// One file's diff with its header, side by side on a wide screen, and a bar to step between files and stage it; Options
/// steps through its changes and discards it. A working or staged diff follows the tree, so it updates while an agent writes.
@MainActor
struct DiffScreen: View {
    let host: HostModel
    let tree: String
    let at: DiffAt
    let files: DiffFiles
    /// The agent whose chat opened the screen, which takes its quotes.
    let chat: String?
    /// Opens another feature's screen: the chat a quote went to, or New Session for a tree without one.
    let open: ((Route) -> Void)?
    @State private var path: String
    @State private var doc: DiffDocument?
    @State private var error: String?
    @State private var ignoreWhitespace = false
    /// Unchanged lines around each change, grown by tapping a hunk header; nil is git's 3.
    @State private var context: Int?
    /// The old-side line to bring into view once the grown diff lands.
    @State private var reveal: Int?
    @State private var change: Int?
    @State private var target: Int?
    @State private var discarding = false
    /// A quote waiting for the user to pick which of these sessions takes it.
    @State private var asking: (quote: String, sessions: [ChatSession])?
    @State private var askable = false
    @State private var toast: Toast?
    @AppStorage("git.diff.sideBySide") private var sideBySide = true
    /// Nil until set, so wrap follows the width: on when compact.
    @AppStorage("diff.wrap") private var wrapSet: Bool?
    @Environment(\.dismiss) private var dismiss
    #if os(iOS)
        @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    init(host: HostModel, tree: String, path: String, at: DiffAt, files: DiffFiles, chat: String? = nil, open: ((Route) -> Void)? = nil) {
        self.host = host
        self.tree = tree
        self.at = at
        self.files = files
        self.chat = chat
        self.open = open
        _path = State(initialValue: path)
        _doc = State(initialValue: host.git.cachedDiff(tree, path: path, at: at))
    }

    var body: some View {
        let _ = BodyCount.hit("DiffScreen")
        let rev = host.git.changes(tree).head?.rev
        let siblings = siblings
        let index = siblings.firstIndex(of: path)
        GeometryReader { geo in
            VStack(spacing: 0) {
                DiffHeader(path: path, doc: doc, status: status)
                Divider()
                content(wide: geo.size.width >= 700)
                    .safeAreaInset(edge: .bottom, spacing: 0) {
                        DiffBar(
                            previous: index.flatMap { $0 > 0 ? { step(-1, siblings) } : nil },
                            next: index.flatMap { $0 + 1 < siblings.count || more ? { step(1, siblings) } : nil },
                            stage: stageTitle, staging: { stage(siblings) }, writable: host.link == .online
                        )
                    }
            }
        }
        .screenBackground()
        .navigationTitle(GitText.split(path).name)
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) {
                BarTitle(GitText.split(path).name, detail: GitText.place(index, of: siblings.count, at: at), mono: false, truncation: .tail)
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    if let doc, doc.path == path, !doc.changes.isEmpty {
                        Section(GitText.position(change, of: doc.changes.count)) {
                            Button("Previous Change", systemImage: "arrow.up") { stepChange(-1, doc) }.disabled((change ?? 0) <= 0)
                            Button("Next Change", systemImage: "arrow.down") { stepChange(1, doc) }.disabled((change ?? -1) + 1 >= doc.changes.count)
                        }
                    }
                    Toggle("Wrap Lines", systemImage: "text.alignleft", isOn: Binding(get: { wrap }, set: { wrapSet = $0 }))
                    Toggle("Side by Side When Wide", systemImage: "rectangle.split.2x1", isOn: $sideBySide)
                    Toggle("Ignore Whitespace", systemImage: "space", isOn: $ignoreWhitespace)
                    if at == .working {
                        Button("Discard Changes…", systemImage: "arrow.uturn.backward", role: .destructive) { discarding = true }
                            .disabled(host.link != .online)
                    }
                } label: {
                    Label("Options", systemImage: "ellipsis")
                }
            }
        }
        .task(id: Load(path: path, ignoreWhitespace: ignoreWhitespace, rev: rev, context: context)) {
            do {
                error = nil
                let loaded = try await host.git.diff(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace, context: context)
                doc = loaded
                change = ChangeStep.kept(change, count: loaded.changes.count)
                if let line = reveal {
                    reveal = nil
                    target = loaded.lines.firstIndex { $0.old == line }
                }
            } catch {
                if doc == nil { self.error = ErrorText.describe(error, mac: host.name) }
            }
        }
        .confirmationDialog(
            "Discard changes to \(GitText.split(path).name)?", isPresented: $discarding, titleVisibility: .visible
        ) {
            Button("Discard", role: .destructive) { discard(siblings) }
        } message: {
            Text(GitText.discardFacts([path], files: host.git.changes(tree).files))
        }
        .confirmationDialog(
            "Ask in Chat", isPresented: Binding(get: { asking != nil }, set: { if !$0 { asking = nil } }), titleVisibility: .visible
        ) {
            if let asking {
                ForEach(asking.sessions) { session in
                    Button(session.title) { ask(session, asking.quote) }
                }
            }
        }
        .toast($toast)
        .onAppear {
            askable = open != nil || askTarget != .none
            if follows { host.git.show(tree) }
        }
        .onDisappear { if follows { host.git.hide(tree) } }
    }

    /// A commit never changes; every other comparison moves with the tree.
    private var follows: Bool {
        if case .commit = at { false } else { true }
    }

    private struct Load: Equatable {
        let path: String
        let ignoreWhitespace: Bool
        let rev: String?
        let context: Int?
    }

    /// The file's letter from the tree's status, which a commit's or a comparison's files aren't in.
    private var status: FileStatus? {
        switch at {
        case .working, .staged: host.git.changes(tree).files.first { $0.path == path }?.status
        case .commit, .base: nil
        }
    }

    private var siblings: [String] {
        switch files {
        case .list(let paths): paths
        case .changes(let section): host.git.changes(tree).paths(section)
        case .compare(let against): host.git.compare(tree, against: against).files.map(\.path)
        }
    }

    /// More files wait in pages not loaded yet.
    private var more: Bool {
        switch files {
        case .list: false
        case .changes: host.git.changes(tree).nextCursor != nil
        case .compare(let against): host.git.compare(tree, against: against).nextCursor != nil
        }
    }

    private func loadMore() async {
        switch files {
        case .list: break
        case .changes: await host.git.loadMore(tree)
        case .compare(let against): await host.git.loadCompare(tree, against: against, more: true)
        }
    }

    private var wrap: Bool {
        #if os(iOS)
            wrapSet ?? (sizeClass == .compact)
        #else
            wrapSet ?? false
        #endif
    }

    private var stageTitle: String? {
        switch at {
        case .working: "Stage"
        case .staged: "Unstage"
        default: nil
        }
    }

    private func step(_ by: Int, _ siblings: [String]) {
        if let next = DiffFiles.step(from: path, by: by, in: siblings) { return open(next) }
        guard by > 0, more else { return }
        Task {
            await loadMore()
            if let next = DiffFiles.step(from: path, by: by, in: self.siblings) { open(next) }
        }
    }

    private func open(_ next: String) {
        Haptics.tap()
        path = next
        doc = host.git.cachedDiff(tree, path: next, at: at, ignoreWhitespace: ignoreWhitespace)
        context = nil
        reveal = nil
        change = nil
        target = nil
    }

    private func stepChange(_ by: Int, _ doc: DiffDocument) {
        guard let next = ChangeStep.step(from: change, by: by, count: doc.changes.count) else { return }
        Haptics.tap()
        change = next
        target = doc.changes[next]
    }

    /// Grows the context so every line folded above hunk `i` shows, and keeps the first of them in view.
    private func expand(_ i: Int) {
        guard let doc, let hidden = DiffRows.hidden(above: i, in: doc.lines) else { return }
        Haptics.tap()
        reveal = hidden.lowerBound
        context = (context ?? 3) + hidden.count
    }

    /// Stages the file, or unstages it, and moves on to the next.
    private func stage(_ siblings: [String]) {
        let next = DiffFiles.after(removing: path, in: siblings)
        let staged = at == .working
        let done = path
        Haptics.tap()
        Task { await host.git.stage(tree, paths: [done], staged: staged) }
        if let next { open(next) } else { dismiss() }
    }

    private func discard(_ siblings: [String]) {
        let next = DiffFiles.after(removing: path, in: siblings)
        let done = path
        Haptics.warning()
        Task { await host.git.discard(tree, paths: [done]) }
        if let next { open(next) } else { dismiss() }
    }

    /// Read when asked, not in `body`, so sessions changing elsewhere don't redraw the diff.
    private var askTarget: ChatTarget {
        host.chatTarget(tree, opener: chat)
    }

    /// Ask in Chat on picked lines, as Files has it: to the tree's session, or a new one when it has none.
    private var onAsk: ((CodeQuote) -> Void)? {
        guard askable else { return nil }
        let path = path
        return { quote in
            let text = quote.payload(path: path)
            switch ChatHandoff(askTarget, mac: host.mac, project: host.workspace.place(of: tree)?.project, tree: tree, prompt: text) {
            case .send(let session): ask(session, text)
            case .choose(let sessions): asking = (text, sessions)
            case .start(let route):
                guard let open, let route else { return toast = Toast("No session in this worktree to ask") }
                open(route)
            }
        }
    }

    private func ask(_ session: ChatSession, _ quote: String) {
        host.files.quote(quote, tree: tree, for: session.id)
        Haptics.success()
        toast = .sent(ChatHandoff.added(to: session), chat: .chat(mac: host.mac, agentId: session.id), open: open)
    }

    @ViewBuilder
    private func content(wide: Bool) -> some View {
        if let doc, doc.path == path {
            switch doc.state {
            case .rows:
                VStack(spacing: 0) {
                    DiffView(doc, sideBySide: wide && sideBySide, wrap: wrap, scrollTo: target, onHunk: expand, onAsk: onAsk)
                    if let t = doc.truncated {
                        Text("Showing \(t.shown) of \(t.total) lines")
                            .font(Typography.caption)
                            .foregroundStyle(Palette.text3.color)
                            .frame(maxWidth: .infinity)
                            .padding(Space.s)
                            .background(Palette.fill2.color)
                    }
                }
            case .binary:
                Placeholder("Binary file", systemImage: "doc.zipper", detail: "Its changes can't be shown as text.")
            case .image:
                Placeholder("Image", systemImage: "photo", detail: "Open it in Files to see it.")
            case .tooLarge:
                Placeholder("Too large to diff", systemImage: "doc.badge.ellipsis", detail: "One side is over 16 MB.")
            case .unchanged:
                Placeholder("No changes", systemImage: "checkmark.circle", detail: "Nothing differs in this comparison.")
            }
        } else if let error {
            Placeholder("Couldn't load the diff", systemImage: "exclamationmark.triangle", detail: error)
        } else {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}

/// The file's letter, icon, path or rename, and its counts above its diff.
@MainActor
private struct DiffHeader: View {
    let path: String
    let doc: DiffDocument?
    let status: FileStatus?

    var body: some View {
        HStack(spacing: Space.s) {
            if let status { StatusLetter(status: status) }
            MaterialIcon(path)
            Text(doc?.oldPath.map { "\($0) → \(path)" } ?? path)
                .font(Typography.codeSmall)
                .foregroundStyle(Palette.text2.color)
                .lineLimit(1)
                .truncationMode(.head)
            Spacer(minLength: Space.s)
            if let doc, doc.state == .rows {
                LineCounts(added: doc.added, removed: doc.removed)
                    .accessibilityLabel("\(doc.added) added, \(doc.removed) removed")
            }
        }
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.s)
        .background(Palette.surface.color)
    }
}
