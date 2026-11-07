import CodeView
import DesignSystem
import DiffModel
import Domain
import Stores
import SwiftUI
import Wire

/// The files Next and Previous File step through.
enum DiffFiles: Hashable {
    /// A fixed list, as a commit's.
    case list([String])
    /// A section of the tree's changes, read as they load, pages included.
    case changes(ChangeSection)
    /// A compare's files, read as they load.
    case compare(against: String)

    /// The file `by` steps from `path`; nil past either end.
    static func step(from path: String, by: Int, in siblings: [String]) -> String? {
        guard let i = siblings.firstIndex(of: path), siblings.indices.contains(i + by) else { return nil }
        return siblings[i + by]
    }

    /// The file to show once `path` leaves the list: the next, else the one before.
    static func after(removing path: String, in siblings: [String]) -> String? {
        step(from: path, by: 1, in: siblings) ?? step(from: path, by: -1, in: siblings)
    }
}

/// Which of a diff's changes the bar has stepped to, as an index into them.
enum ChangeStep {
    /// The change `by` steps from `current`; from none, Next takes the first and Previous the last. Nil past either end.
    static func step(from current: Int?, by: Int, count: Int) -> Int? {
        let next = (current ?? (by > 0 ? -1 : count)) + by
        return (0..<count).contains(next) ? next : nil
    }

    /// The change kept once a reloaded diff has `count` of them: the same one, else the last.
    static func kept(_ current: Int?, count: Int) -> Int? {
        guard let current, count > 0 else { return nil }
        return min(current, count - 1)
    }
}

/// One file's diff with its header, next and previous file, side by side on a wide screen, and a bar to step through
/// its changes and stage or discard it. A working or staged diff follows the tree, so it updates while an agent writes.
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
                DiffHeader(path: path, doc: doc, at: at)
                Divider()
                content(wide: geo.size.width >= 700)
                if let doc, doc.path == path {
                    Divider()
                    DiffBar(
                        changes: doc.changes.count, current: change, step: { stepChange($0, doc) }, stage: stageTitle, staging: { stage(siblings) },
                        discard: at == .working ? { discarding = true } : nil, writable: host.link == .online
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
            ToolbarItemGroup(placement: .primaryAction) {
                Button("Previous File", systemImage: "chevron.up") { step(-1, siblings) }.disabled(index.map { $0 == 0 } ?? true)
                Button("Next File", systemImage: "chevron.down") { step(1, siblings) }
                    .disabled(index.map { $0 + 1 >= siblings.count && !more } ?? true)
                Menu {
                    Toggle("Wrap Lines", systemImage: "text.alignleft", isOn: Binding(get: { wrap }, set: { wrapSet = $0 }))
                    Toggle("Side by Side When Wide", systemImage: "rectangle.split.2x1", isOn: $sideBySide)
                    Toggle("Ignore Whitespace", systemImage: "space", isOn: $ignoreWhitespace)
                } label: {
                    Label("Options", systemImage: "ellipsis.circle")
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

    private var siblings: [String] {
        switch files {
        case .list(let paths): paths
        case .changes(let section): host.git.changes(tree).files.filter { section == .staged ? $0.staged : $0.unstaged }.map(\.path)
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
        ChatTarget(ChatSession.on(tree, host.sessions.sessions.values.map(\.summary)), opener: chat)
    }

    /// Ask in Chat on picked lines, as Files has it: to the tree's session, or a new one when it has none.
    private var onAsk: ((CodeQuote) -> Void)? {
        guard askable else { return nil }
        let path = path
        return { quote in
            let text = quote.payload(path: path)
            switch askTarget {
            case .one(let session): ask(session, text)
            case .choose(let sessions): asking = (text, sessions)
            case .none:
                guard let open else { return toast = Toast("No session in this worktree to ask") }
                open(.newSession(mac: host.mac, project: host.workspace.place(of: tree)?.project, place: tree, prompt: text))
            }
        }
    }

    private func ask(_ session: ChatSession, _ quote: String) {
        host.files.quote(quote, tree: tree, for: session.id)
        Haptics.success()
        let done = "Added to \(session.agent)'s input"
        guard let open else { return toast = Toast(done) }
        let chat = Route.chat(mac: host.mac, agentId: session.id)
        toast = Toast(done, actionTitle: "Go to chat") { open(chat) }
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

/// The file's name, folder, rename and counts above its diff.
@MainActor
private struct DiffHeader: View {
    let path: String
    let doc: DiffDocument?
    let at: DiffAt

    var body: some View {
        let (name, dir) = GitText.split(path)
        HStack(alignment: .firstTextBaseline, spacing: Space.s) {
            FileChip(path)
            VStack(alignment: .leading, spacing: 2) {
                if let old = doc?.oldPath {
                    Text("Renamed from \(old)").font(Typography.caption).foregroundStyle(Palette.link.color).lineLimit(1).truncationMode(.head)
                } else if !dir.isEmpty {
                    Text(dir).font(Typography.caption).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                }
                Text(GitText.comparison(at)).font(Typography.caption).foregroundStyle(Palette.text4.color)
            }
            .accessibilityLabel("\(name) \(dir)")
            Spacer(minLength: Space.s)
            if let doc, doc.state == .rows {
                HStack(spacing: Space.xs) {
                    Text("+\(doc.added)").foregroundStyle(Palette.diffAddText.color)
                    Text("−\(doc.removed)").foregroundStyle(Palette.diffDelText.color)
                }
                .font(Typography.digits)
                .accessibilityLabel("\(doc.added) added, \(doc.removed) removed")
            }
        }
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.s)
        .background(Palette.surface.color)
    }
}
