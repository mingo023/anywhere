import DesignSystem
import DiffModel
import Domain
import Stores
import SwiftUI
import Wire

/// A tree's changes: its branch, the commit composer, then Staged and Changes, flat or as a tree.
///
/// The files are a lazy stack of fixed-height rows rather than a `List`, which rediffs every row on each status read
/// (~300 ms a refresh at 1500 files on the Mac). Lines are made once per read and fold, and each row reads only its own
/// selection and swipe state.
@MainActor
struct ChangesScreen: View {
    let host: HostModel
    let mac: String
    let tree: String
    @AppStorage("git.changes.tree") private var asTree = false
    @State private var folded: Set<String> = []
    @State private var selection = ChangeSelection()
    @State private var swiped = SwipeOpen()
    @State private var lines = ChangeLines()
    @State private var discarding: [String]?

    var body: some View {
        let _ = BodyCount.hit("ChangesScreen")
        let store = host.git
        let changes = store.changes(tree)
        let lines = lines.lines(changes, tree: asTree, folded: folded)
        let actions = ChangeActions(
            fold: { ChangeTree.toggleFold(&folded, $0) }, stage: { stage($0, staged: $1) }, discard: { discarding = $0 })
        ScrollView {
            LazyVStack(spacing: 0) {
                VStack(spacing: Space.l) {
                    if let head = changes.head {
                        HeadRow(store: store, changes: changes, head: head, mac: mac).card()
                    } else if changes.loading {
                        HStack { ProgressView(); Text("Reading changes…").foregroundStyle(Palette.text3.color) }.card()
                    }
                    if let error = changes.error {
                        ErrorRow(message: error) { changes.error = nil }.card()
                    }
                    if changes.head != nil {
                        CommitComposer(store: store, changes: changes).card()
                    }
                    if changes.head != nil, changes.files.isEmpty {
                        Label("No changes", systemImage: "checkmark.circle").foregroundStyle(Palette.text3.color).card()
                    }
                }
                .padding(.top, Space.l)
                ForEach(lines) { line in
                    ChangeLineView(
                        line: line, mac: mac, tree: tree, nameOnly: asTree, folded: line.header.map { folded.contains($0.rawValue) } ?? false,
                        selection: selection, swiped: swiped, actions: actions
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
        .screenBackground()
        #if os(iOS)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button(selection.selecting ? "Done" : "Edit") {
                        swiped.close()
                        selection.selecting.toggle()
                        if !selection.selecting { selection.ids = [] }
                    }
                    .fontWeight(selection.selecting ? .semibold : .regular)
                }
                ToolbarItemGroup(placement: .bottomBar) {
                    SelectionBar(selection: selection, stage: { stage($0, staged: $1) }, discard: { discarding = $0 })
                }
            }
        #endif
        .toolbar {
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Picker("View", selection: $asTree) {
                        Label("List", systemImage: "list.bullet").tag(false)
                        Label("Tree", systemImage: "list.bullet.indent").tag(true)
                    }
                    NavigationLink(value: Route.history(mac: mac, tree: tree)) { Label("History", systemImage: "clock.arrow.circlepath") }
                } label: {
                    Label("Options", systemImage: "ellipsis.circle")
                }
            }
        }
        .navigationTitle("Changes")
        .refreshable { await store.refresh(tree) }
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
            Text("This can't be undone.")
        }
        .onAppear { store.show(tree) }
        .onDisappear { store.hide(tree) }
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
}

/// What a row asks of the screen.
@MainActor
struct ChangeActions {
    let fold: (String) -> Void
    let stage: (_ paths: [String], _ staged: Bool) -> Void
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
                Button("Discard", role: .destructive) { discard(changed) }.disabled(changed.isEmpty)
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
    let nameOnly: Bool
    let folded: Bool
    let selection: ChangeSelection
    let swiped: SwipeOpen
    let actions: ChangeActions
    // A grouped list's row heights: two lines of name and folder, or one.
    #if os(iOS)
        @ScaledMetric(relativeTo: .body) private var fileHeight: CGFloat = 58
        @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 44
    #else
        @ScaledMetric(relativeTo: .body) private var fileHeight: CGFloat = 38
        @ScaledMetric(relativeTo: .body) private var rowHeight: CGFloat = 28
    #endif

    nonisolated static func == (a: Self, b: Self) -> Bool {
        a.line == b.line && a.nameOnly == b.nameOnly && a.folded == b.folded && a.mac == b.mac && a.tree == b.tree
    }

    var body: some View {
        switch line.row {
        case .header(let section, let paths):
            SectionHeader(
                section: section, count: paths.count, folded: folded, fold: { actions.fold(section.rawValue) },
                stageAll: { actions.stage(paths, section == .changes) })
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
                    Button(staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle") { actions.stage([f.path], !staged) }
                    if !staged {
                        Button("Discard Changes…", systemImage: "arrow.uturn.backward", role: .destructive) { actions.discard([f.path]) }
                    }
                    Button("Copy Path", systemImage: "doc.on.doc") { Pasteboard.copy(f.path) }
                }
                .cell(line)
        }
    }

    @ViewBuilder
    private func file(_ section: ChangeSection, _ f: GitFile, depth: Int) -> some View {
        let row = FileRow(
            path: f.path, oldPath: f.oldPath, status: f.status, counts: GitText.counts(added: f.added, removed: f.removed, binary: f.binary == true),
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
            NavigationLink(value: Route.diff(mac: mac, tree: tree, path: f.path, staged: section == .staged)) {
                HStack(spacing: Space.s) {
                    row
                    #if os(iOS)
                        Image(systemName: "chevron.right").font(.footnote.weight(.semibold)).foregroundStyle(Palette.text5.color)
                    #endif
                }
                .padding(.horizontal, Space.l)
                .frame(maxHeight: .infinity)
                .contentShape(Rectangle())
            }
            .buttonStyle(RowPress())
        }
    }

    private func stageAction(_ section: ChangeSection, _ f: GitFile) -> SwipeAction? {
        guard !selection.selecting else { return nil }
        let staged = section == .staged
        return SwipeAction(
            title: staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle",
            tint: (staged ? Palette.text3 : Palette.success).color
        ) { actions.stage([f.path], !staged) }
    }

    private func discardAction(_ section: ChangeSection, _ f: GitFile) -> SwipeAction? {
        guard !selection.selecting, section == .changes else { return nil }
        return SwipeAction(title: "Discard", systemImage: "arrow.uturn.backward", tint: Palette.failed.color) { actions.discard([f.path]) }
    }
}

extension ChangeLine {
    fileprivate var header: ChangeSection? {
        if case .header(let s, _) = row { s } else { nil }
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

/// The branch, its distance from the base and upstream, and Push and Pull.
@MainActor
private struct HeadRow: View {
    let store: GitStore
    let changes: TreeChanges
    let head: GitHead
    let mac: String

    var body: some View {
        VStack(alignment: .leading, spacing: Space.m) {
            BranchIndicator(head: head)
            HStack(spacing: Space.s) {
                if let push = GitText.push(head) {
                    Button(push, systemImage: "arrow.up") { run { await store.push(changes.tree) } }
                        .buttonStyle(.bordered)
                }
                if head.upstream != nil {
                    Button("Pull", systemImage: "arrow.down") { run { await store.pull(changes.tree) } }
                        .buttonStyle(.bordered)
                }
                Spacer()
                Text("\(head.totals.files) files · +\(head.totals.added) −\(head.totals.removed)")
                    .font(Typography.digits)
                    .foregroundStyle(Palette.text3.color)
            }
            .disabled(changes.busy != nil)
            .controlSize(.small)
            if !head.commits.isEmpty {
                DisclosureGroup("\(head.commits.count) commit\(head.commits.count == 1 ? "" : "s") since \(head.base ?? "base")") {
                    ForEach(head.commits, id: \.sha) { c in
                        NavigationLink(value: Route.commit(mac: mac, tree: changes.tree, sha: c.sha)) {
                            HStack {
                                Text(GitText.short(c.sha)).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                                Text(c.subject).lineLimit(1)
                            }
                        }
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

/// A section's title with its count, a fold chevron and Stage All or Unstage All.
@MainActor
private struct SectionHeader: View {
    let section: ChangeSection
    let count: Int
    let folded: Bool
    let fold: () -> Void
    let stageAll: () -> Void

    var body: some View {
        HStack {
            Button(action: fold) {
                HStack(spacing: Space.xs) {
                    Image(systemName: "chevron.right").rotationEffect(.degrees(folded ? 0 : 90)).font(.caption2.weight(.bold))
                    Text(section.title)
                    Text("\(count)").monospacedDigit().foregroundStyle(Palette.text3.color)
                }
            }
            .buttonStyle(.plain)
            .accessibilityValue(folded ? "Folded" : "Open")
            Spacer()
            Button(section == .staged ? "Unstage All" : "Stage All", action: stageAll)
                .font(Typography.caption)
                .buttonStyle(.borderless)
        }
        .font(Typography.footnote)
        .foregroundStyle(Palette.text2.color)
        .padding(.horizontal, Space.l * 2)
        .padding(.top, Space.xl)
        .padding(.bottom, Space.s)
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
            Button("Dismiss", systemImage: "xmark", action: dismiss).labelStyle(.iconOnly).buttonStyle(.borderless)
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
