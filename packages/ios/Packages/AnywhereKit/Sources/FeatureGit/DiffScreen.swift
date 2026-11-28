import CodeView
import DesignSystem
import DiffModel
import Domain
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
    /// Files marked as read on this screen; the Mac keeps no such mark.
    @State private var viewed: Set<String> = []
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
        let counts = totals
        GeometryReader { geo in
            VStack(spacing: 0) {
                HStack(spacing: 0) {
                    FileHeaderLead(
                        title: doc?.oldPath.map { "\($0) → \(path)" } ?? GitText.shortPath(path), open: true,
                        added: doc?.state == .rows ? doc?.added : nil, removed: doc?.state == .rows ? doc?.removed : nil)
                    FileHeaderTail(viewed: viewed.contains(path), toggleViewed: toggleViewed) { fileMenu(siblings) }
                }
                .fileHeaderBar()
                content(wide: geo.size.width >= 700)
                    .safeAreaInset(edge: .bottom, spacing: 0) {
                        DiffBar(
                            previous: index.flatMap { $0 > 0 ? { step(-1, siblings) } : nil },
                            next: index.flatMap { $0 + 1 < siblings.count || more ? { step(1, siblings) } : nil },
                            place: GitText.fileOf(index, of: siblings.count),
                            step: hasNextChange || index.map({ $0 + 1 < siblings.count || more }) == true ? { nextChange(siblings) } : nil
                        )
                    }
            }
        }
        .screenBackground()
        .navigationTitle("Files Changed")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) {
                BarHeading("Files Changed") {
                    if let counts {
                        HStack(spacing: Space.s) {
                            Text("+\(counts.added)").foregroundStyle(Palette.successText.color)
                            Text("−\(counts.removed)").foregroundStyle(Palette.failedText.color)
                        }
                    }
                }
            }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Toggle(isOn: Binding(get: { wrap }, set: { wrapSet = $0 })) { Label("Wrap Lines", lucide: .textWrap) }
                    Toggle(isOn: $sideBySide) { Label("Side by Side When Wide", lucide: .splitRight) }
                    Toggle(isOn: $ignoreWhitespace) { Label("Ignore Whitespace", lucide: .eyeOff) }
                } label: {
                    BarMenuLabel()
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

    /// Lines added and removed across the files the bar steps through: the tree's own, the comparison's or the commit's.
    private var totals: (added: Int, removed: Int)? {
        switch files {
        case .changes: return host.git.changes(tree).head.map { ($0.totals.added, $0.totals.removed) }
        case .compare(let against): return host.git.compare(tree, against: against).totals.map { ($0.added, $0.removed) }
        case .list:
            guard case .commit(let rev) = at, let commit = host.git.cachedCommit(rev) else { return nil }
            return (commit.files.reduce(0) { $0 + ($1.added ?? 0) }, commit.files.reduce(0) { $0 + ($1.removed ?? 0) })
        }
    }

    private var hasNextChange: Bool {
        guard let doc, doc.path == path else { return false }
        return ChangeStep.step(from: change, by: 1, count: doc.changes.count) != nil
    }

    private func toggleViewed() {
        Haptics.tap()
        if viewed.remove(path) == nil { viewed.insert(path) }
    }

    /// The next change of this file, then the first of the next.
    private func nextChange(_ siblings: [String]) {
        if hasNextChange, let doc { stepChange(1, doc) } else { step(1, siblings) }
    }

    @ViewBuilder
    private func fileMenu(_ siblings: [String]) -> some View {
        if let doc, doc.path == path, !doc.changes.isEmpty {
            Section(GitText.position(change, of: doc.changes.count)) {
                Button { stepChange(-1, doc) } label: { Label("Previous Change", lucide: .arrowUp) }.disabled((change ?? 0) <= 0)
                Button { stepChange(1, doc) } label: { Label("Next Change", lucide: .arrowDown) }.disabled(!hasNextChange)
            }
        }
        if let stageTitle {
            Button { stage(siblings) } label: { Label(stageTitle, lucide: stageTitle == "Stage" ? .plus : .minus) }.disabled(host.link != .online)
        }
        if at == .working {
            Button(role: .destructive) { discarding = true } label: { Label("Discard Changes…", lucide: .discard) }.disabled(host.link != .online)
        }
        Button { Pasteboard.copy(path) } label: { Label("Copy Path", lucide: .copy) }
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
                        Text("Showing \(t.shown) of \(t.total) lines.")
                            .font(.system(size: 15))
                            .foregroundStyle(Palette.text2.color)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .padding(Space.l)
                    }
                }
            case .binary:
                Placeholder("Binary file", icon: .package, detail: "Its changes can't be shown as text.")
            case .image:
                Placeholder("Image", icon: .imagePlus, detail: "Open it in Files to see it.")
            case .tooLarge:
                Placeholder("Too large to diff", icon: .file, detail: "One side is over 16 MB.")
            case .unchanged:
                Placeholder("No changes", icon: .circleCheck, detail: "Nothing differs in this comparison.")
            }
        } else if let error {
            Placeholder("Couldn't load the diff", icon: .triangleAlert, detail: error)
        } else {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }
}
