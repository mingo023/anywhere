import DesignSystem
import DiffModel
import Domain
import Stores
import SwiftUI
import Wire

/// A tree's changes: its branch, then Staged and Unstaged, flat or as a tree, over the commit composer; or, in the Compare
/// scope, what the branch committed since it left its base.
///
/// The files are a lazy stack of fixed-height rows rather than a `List`, which rediffs every row on each status read
/// (~300 ms a refresh at 1500 files on the Mac). Lines are made once per read and fold, and each row reads only its own
/// selection and swipe state.
@MainActor
struct ChangesScreen: View {
    let host: HostModel
    let mac: String
    let tree: String
    /// The agent whose chat opened Changes, which takes quotes from its diffs.
    let chat: String?
    let open: ((Route) -> Void)?
    @AppStorage("git.changes.tree") private var asTree = false
    /// The branch this worktree's Compare is against; nil is its base.
    @AppStorage private var picked: String?
    @State private var comparing = false
    @State private var choosing = false
    @State private var folded: Set<String> = []
    @State private var selection = ChangeSelection()
    @State private var swiped = SwipeOpen()
    @State private var lines = ChangeLines()
    @State private var filter = ""
    @State private var discarding: [String]?

    init(host: HostModel, mac: String, tree: String, chat: String? = nil, open: ((Route) -> Void)? = nil) {
        self.host = host
        self.mac = mac
        self.tree = tree
        self.chat = chat
        self.open = open
        _picked = AppStorage("git.compare.\(tree)")
    }

    var body: some View {
        let _ = BodyCount.hit("ChangesScreen")
        let store = host.git
        let changes = store.changes(tree)
        let offline = host.link != .online
        let against = host.hello?.has(Caps.compare) == true ? picked ?? changes.head?.base : nil
        Group {
            if changes.head == nil, !changes.loading, offline || changes.error != nil {
                Placeholder(offline ? "Can't reach \(host.name)" : "Couldn't read the changes", systemImage: offline ? "wifi.slash" : "exclamationmark.triangle",
                            detail: offline ? nil : changes.error) {
                    changes.error = nil
                    Task { await store.refresh(tree) }
                }
            } else if comparing, let against {
                CompareList(host: host, tree: tree, against: against) { if picked != nil { picked = nil } }
            } else {
                uncommitted(changes, writable: !offline)
            }
        }
        .screenBackground()
        .safeAreaInset(edge: .top, spacing: 0) {
            VStack(spacing: 0) {
                if offline, changes.head != nil {
                    StaleStrip(mac: host.name, asOf: host.offlineSince.map { Int($0.timeIntervalSince1970 * 1000) })
                }
                if let against, changes.head != nil {
                    ScopeBar(comparing: $comparing, against: against, picked: picked != nil, choose: { choosing = true }, reset: { picked = nil })
                }
            }
        }
        #if os(iOS)
            .toolbar {
                if !comparing {
                    if selection.selecting {
                        ToolbarItem(placement: .topBarTrailing) { Button("Done", action: toggleSelecting).fontWeight(.semibold) }
                    }
                    ToolbarItemGroup(placement: .bottomBar) {
                        SelectionBar(selection: selection, stage: { stage($0, staged: $1) }, discard: { discarding = $0 }).disabled(offline)
                    }
                }
            }
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) { BarTitle("Changes", detail: changes.head?.branch) }
            // One item, as wide as Back: a control beside it nests in the bar's glass and squeezes the title.
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Picker("View", selection: $asTree) {
                        Label("List", systemImage: "list.bullet").tag(false)
                        Label("Tree", systemImage: "list.bullet.indent").tag(true)
                    }
                    .pickerStyle(.inline)
                    #if os(iOS)
                        if !comparing { Button("Select Files", systemImage: "checkmark.circle", action: toggleSelecting) }
                    #endif
                    NavigationLink(value: Route.history(mac: mac, tree: tree)) { Label("History", systemImage: "clock.arrow.circlepath") }
                } label: {
                    Label("Options", systemImage: "ellipsis")
                }
            }
        }
        .navigationTitle("Changes")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .safeAreaInset(edge: .bottom, spacing: 0) {
            if !comparing, !selection.selecting, changes.head != nil {
                CommitComposer(store: store, changes: changes).disabled(offline)
            }
        }
        .navigationDestination(for: CompareFileRoute.self) { r in
            DiffScreen(host: host, tree: r.tree, path: r.path, at: .base(rev: r.against), files: .compare(against: r.against), chat: chat, open: open)
        }
        .sheet(isPresented: $choosing) {
            ComparePicker(host: host, tree: tree, base: changes.head?.base, picked: $picked)
        }
        .confirmationDialog(
            discardTitle, isPresented: Binding(get: { discarding != nil }, set: { if !$0 { discarding = nil } }), titleVisibility: .visible
        ) {
            Button("Discard", role: .destructive) {
                guard let paths = discarding else { return }
                selection.ids = []
                Haptics.warning()
                Task { await store.discard(tree, paths: paths) }
            }
        } message: {
            Text(GitText.discardFacts(discarding ?? [], files: changes.files))
        }
        .onChange(of: picked) { if picked != nil { comparing = true } }
        .onAppear { store.show(tree) }
        .onDisappear { store.hide(tree) }
    }

    private func uncommitted(_ changes: TreeChanges, writable: Bool) -> some View {
        let store = host.git
        let lines = lines.lines(changes, tree: asTree, folded: folded, filter: filter)
        let actions = ChangeActions(
            fold: { ChangeTree.toggleFold(&folded, $0) }, stage: { stage($0, staged: $1) }, stageAll: { stageAll($0) }, discard: { discarding = $0 })
        let busy = changes.busy != nil || changes.bulk != nil || !writable
        let filtering = !filter.trimmingCharacters(in: .whitespaces).isEmpty
        return ScrollView {
            LazyVStack(spacing: 0) {
                VStack(spacing: Space.l) {
                    SearchField("Filter \(changes.files.count.formatted()) files", text: $filter).padding(.horizontal, Space.l)
                    if let head = changes.head, GitText.showsHead(head) {
                        HeadRow(store: store, changes: changes, head: head, mac: mac).card().disabled(!writable)
                    } else if changes.head == nil, changes.loading {
                        HStack { ProgressView(); Text("Reading changes…").foregroundStyle(Palette.text3.color) }.card()
                    }
                    if let error = changes.error {
                        ErrorRow(message: error) { changes.error = nil }.card()
                    }
                    if changes.head != nil, changes.files.isEmpty {
                        Label("No changes", systemImage: "checkmark.circle").foregroundStyle(Palette.text3.color).card()
                    }
                }
                .padding(.top, Space.s)
                ForEach(lines) { line in
                    let header = line.header
                    ChangeLineView(
                        line: line, mac: mac, tree: tree, chat: chat, nameOnly: asTree, folded: header.map { folded.contains($0.section.rawValue) } ?? false,
                        stageAll: filtering ? nil : header.map { GitText.stageAll($0.section, bulk: changes.bulk) },
                        count: header.map { filtering ? $0.paths.count : changes.count($0.section) } ?? 0,
                        writable: writable, busy: busy, selection: selection, swiped: swiped, actions: actions
                    )
                    .equatable()
                }
                if changes.nextCursor != nil {
                    ProgressView().frame(maxWidth: .infinity).padding(.vertical, Space.l)
                        .onAppear { Task { await store.loadMore(tree) } }
                }
            }
            .padding(.bottom, Space.xl)
        }
        .refreshable { await store.refresh(tree) }
    }

    private func toggleSelecting() {
        swiped.close()
        selection.selecting.toggle()
        if !selection.selecting { selection.ids = [] }
    }

    private var discardTitle: String {
        let n = discarding?.count ?? 0
        return n == 1 ? "Discard changes to \(GitText.split(discarding![0]).name)?" : "Discard changes to \(n) files?"
    }

    private func stage(_ paths: [String], staged: Bool) {
        guard !paths.isEmpty else { return }
        Haptics.tap()
        selection.ids = []
        Task { await host.git.stage(tree, paths: paths, staged: staged) }
    }

    private func stageAll(_ section: ChangeSection) {
        Haptics.tap()
        Task { await host.git.stageAll(tree, section: section) }
    }
}

/// What a row asks of the screen.
@MainActor
struct ChangeActions {
    let fold: (String) -> Void
    let stage: (_ paths: [String], _ staged: Bool) -> Void
    let stageAll: (ChangeSection) -> Void
    let discard: ([String]) -> Void
}

#if os(iOS)
    /// Edit mode's Stage, Unstage and Discard for the picked rows; its own view, so picking redraws only it.
    @MainActor
    private struct SelectionBar: View {
        let selection: ChangeSelection
        let stage: (_ paths: [String], _ staged: Bool) -> Void
        let discard: ([String]) -> Void

        var body: some View {
            if !selection.ids.isEmpty {
                let changed = ChangeLine.paths(selection.ids, in: .changes)
                let staged = ChangeLine.paths(selection.ids, in: .staged)
                Button("Stage \(changed.count)") { stage(changed, true) }.disabled(changed.isEmpty)
                Spacer()
                Button("Unstage \(staged.count)") { stage(staged, false) }.disabled(staged.isEmpty)
                Spacer()
                Button("Discard \(changed.count)", role: .destructive) { discard(changed) }.disabled(changed.isEmpty)
            }
        }
    }
#endif

/// One line of the list: a section's header, a folder of the tree or a file, at a fixed height. Equal lines skip their
/// body, so a status read redraws only the rows that changed.
@MainActor
private struct ChangeLineView: View, Equatable {
    let line: ChangeLine
    let mac: String
    let tree: String
    let chat: String?
    let nameOnly: Bool
    let folded: Bool
    /// A header's Stage All or Unstage All; nil while a filter hides rows it would still stage.
    let stageAll: String?
    /// A header's file count: the filter's matches, else the whole tree's.
    let count: Int
    /// Stage and Discard are offered; not while the Mac is offline.
    let writable: Bool
    /// A write is under way, so Stage All waits.
    let busy: Bool
    let selection: ChangeSelection
    let swiped: SwipeOpen
    let actions: ChangeActions
    // A grouped list's row heights: two lines of name and folder, or one.
    #if os(iOS)
        @ScaledMetric(relativeTo: .body) private var fileHeight: CGFloat = 50
        @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 40
    #else
        @ScaledMetric(relativeTo: .body) private var fileHeight: CGFloat = 38
        @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 28
    #endif

    nonisolated static func == (a: Self, b: Self) -> Bool {
        a.line == b.line && a.nameOnly == b.nameOnly && a.folded == b.folded && a.mac == b.mac && a.tree == b.tree && a.chat == b.chat && a.stageAll == b.stageAll
            && a.count == b.count && a.writable == b.writable && a.busy == b.busy
    }

    var body: some View {
        switch line.row {
        case .header(let section, _):
            SectionHeader(
                section: section, count: count, folded: folded, stageAll: stageAll, busy: busy, fold: { actions.fold(section.rawValue) },
                stage: { actions.stageAll(section) }
            )
        case .folder(_, let key, let label, let depth, let open):
            Button {
                actions.fold(key)
            } label: {
                FolderRow(label: label, depth: depth, open: open).padding(.horizontal, Space.l).frame(height: rowHeight)
            }
            .buttonStyle(RowPress())
            .cell(line)
        case .file(let section, let f, let depth):
            file(section, f, depth: depth)
                .frame(height: f.oldPath == nil && (nameOnly || GitText.split(f.path).dir.isEmpty) ? rowHeight : fileHeight)
                .swipe(line.id, open: swiped, leading: stageAction(section, f), trailing: discardAction(section, f))
                .contextMenu {
                    let staged = section == .staged
                    if writable {
                        Button(staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle") { actions.stage([f.path], !staged) }
                        if !staged {
                            Button("Discard Changes…", systemImage: "arrow.uturn.backward", role: .destructive) { actions.discard([f.path]) }
                        }
                    }
                    Button("Copy Path", systemImage: "doc.on.doc") { Pasteboard.copy(f.path) }
                }
                .cell(line)
        }
    }

    @ViewBuilder
    private func file(_ section: ChangeSection, _ f: GitFile, depth: Int) -> some View {
        let row = FileRow(
            path: f.path, oldPath: f.oldPath, status: f.status, added: f.added, removed: f.removed, binary: f.binary == true,
            depth: depth, nameOnly: nameOnly)
        if selection.selecting {
            let picked = selection.ids.contains(line.id)
            Button {
                selection.toggle(line.id)
            } label: {
                HStack(spacing: Space.m) {
                    Image(systemName: picked ? "checkmark.circle.fill" : "circle")
                        .font(.title3)
                        .foregroundStyle((picked ? Palette.link : Palette.text4).color)
                    row
                }
                .padding(.horizontal, Space.l)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityAddTraits(picked ? .isSelected : [])
        } else {
            NavigationLink(value: Route.diff(mac: mac, tree: tree, path: f.path, staged: section == .staged, chat: chat)) {
                row
                .padding(.horizontal, Space.l)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(RowPress())
        }
    }

    private func stageAction(_ section: ChangeSection, _ f: GitFile) -> SwipeAction? {
        guard !selection.selecting, writable else { return nil }
        let staged = section == .staged
        return SwipeAction(
            title: staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle",
            tint: (staged ? Palette.text3 : Palette.success).color
        ) { actions.stage([f.path], !staged) }
    }

    private func discardAction(_ section: ChangeSection, _ f: GitFile) -> SwipeAction? {
        guard !selection.selecting, writable, section == .changes else { return nil }
        return SwipeAction(title: "Discard", systemImage: "arrow.uturn.backward", tint: Palette.failed.color) { actions.discard([f.path]) }
    }
}

extension ChangeLine {
    fileprivate var header: (section: ChangeSection, paths: [String])? {
        if case .header(let s, let paths) = row { (s, paths) } else { nil }
    }
}

/// A grouped list's row press: the row greys while held.
private struct RowPress: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label.background(configuration.isPressed ? Palette.fill3.color : .clear)
    }
}

extension View {
    /// A row of a section's card: the card's surface, its rounded ends and the separator under all but the last.
    fileprivate func cell(_ line: ChangeLine) -> some View {
        let r = Radius.card
        return background(Palette.surface.color)
            .overlay(alignment: .bottom) {
                if !line.last { Palette.separator.color.frame(height: 1).padding(.leading, Space.l + 16 + Space.s) }
            }
            .clipShape(UnevenRoundedRectangle(
                topLeadingRadius: line.first ? r : 0, bottomLeadingRadius: line.last ? r : 0, bottomTrailingRadius: line.last ? r : 0,
                topTrailingRadius: line.first ? r : 0, style: .continuous))
            .padding(.horizontal, Space.l)
    }

    /// One of the cards over the files: the branch, the composer, an error.
    fileprivate func card() -> some View {
        padding(.horizontal, Space.l)
            .padding(.vertical, Space.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .groupedCard()
    }
}

/// The branch against its base, Push and Pull against the upstream, and the commits since the base.
@MainActor
private struct HeadRow: View {
    let store: GitStore
    let changes: TreeChanges
    let head: GitHead
    let mac: String
    @State private var forcing = false

    var body: some View {
        let force = GitText.forcePush(head)
        VStack(alignment: .leading, spacing: Space.m) {
            BranchIndicator(head: head)
            HStack(spacing: Space.s) {
                if let push = GitText.push(head) {
                    Button(push, systemImage: "arrow.up") {
                        if force != nil { forcing = true } else { run { await store.push(changes.tree) } }
                    }
                    .buttonStyle(.bordered)
                    .confirmationDialog(force?.title ?? "", isPresented: $forcing, titleVisibility: .visible) {
                        Button("Force Push", role: .destructive) { run { await store.push(changes.tree, lease: true) } }
                    } message: {
                        Text(force?.message ?? "")
                    }
                }
                if let pull = GitText.pull(head) {
                    Button(pull, systemImage: "arrow.down") { run { await store.pull(changes.tree) } }
                        .buttonStyle(.bordered)
                }
                Spacer()
                Text("\(head.totals.files) files · +\(head.totals.added) −\(head.totals.removed)")
                    .font(Typography.digits)
                    .foregroundStyle(Palette.text3.color)
            }
            .disabled(changes.busy != nil)
            if head.base != nil, head.ahead > 0 {
                DisclosureGroup(GitText.sinceBase(head)) {
                    ForEach(head.commits, id: \.sha) { c in
                        NavigationLink(value: Route.commit(mac: mac, tree: changes.tree, sha: c.sha)) {
                            HStack {
                                Text(GitText.short(c.sha)).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                                Text(c.subject).lineLimit(1)
                            }
                            .frame(minHeight: HitTarget.minimum)
                        }
                    }
                    NavigationLink(value: Route.history(mac: mac, tree: changes.tree)) {
                        Text("Show all in History").foregroundStyle(Palette.link.color).frame(minHeight: HitTarget.minimum)
                    }
                }
                .font(Typography.subheadline)
            }
        }
        .padding(.vertical, Space.xs)
    }

    private func run(_ work: @escaping @MainActor () async -> Bool) {
        Haptics.tap()
        Task {
            if await work() { Haptics.success() } else { Haptics.warning() }
        }
    }
}

/// A section's title and count, which fold it, and Stage All or Unstage All.
@MainActor
private struct SectionHeader: View {
    let section: ChangeSection
    let count: Int
    let folded: Bool
    let stageAll: String?
    let busy: Bool
    let fold: () -> Void
    let stage: () -> Void

    var body: some View {
        HStack {
            Button(action: fold) {
                HStack(spacing: Space.xs) {
                    Text(section.title).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.text.color)
                    Text(count.formatted()).monospacedDigit().foregroundStyle(Palette.text3.color)
                }
                .frame(minHeight: HitTarget.minimum)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .accessibilityValue(folded ? "Folded" : "Open")
            Spacer()
            if let stageAll {
                Button(action: stage) {
                    Text(stageAll).monospacedDigit().hitTarget()
                }
                .font(Typography.footnote.weight(.semibold))
                .foregroundStyle(Palette.text.color)
                .buttonStyle(.plain)
                .disabled(busy)
            }
        }
        .font(Typography.footnote)
        .foregroundStyle(Palette.text2.color)
        .padding(.horizontal, Space.l * 2)
        .padding(.top, Space.m)
    }
}

@MainActor
struct ErrorRow: View {
    let message: String
    let dismiss: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: Space.s) {
            Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(Palette.failed.color)
            Text(message).font(Typography.subheadline).foregroundStyle(Palette.failedText.color).textSelection(.enabled)
            Spacer()
            Button(action: dismiss) { Image(systemName: "xmark").hitTarget() }
                .buttonStyle(.borderless)
                .accessibilityLabel("Dismiss")
        }
    }
}

enum Pasteboard {
    @MainActor static func copy(_ text: String) {
        #if os(iOS)
            UIPasteboard.general.string = text
        #else
            NSPasteboard.general.clearContents()
            NSPasteboard.general.setString(text, forType: .string)
        #endif
    }
}
