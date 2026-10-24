import DesignSystem
import DiffModel
import Domain
import Stores
import SwiftUI
import Wire

/// A run of rows under one section header.
struct SectionRows: Identifiable, Hashable {
    let section: ChangeSection
    let paths: [String]
    let rows: [ChangeRow]
    var id: ChangeSection { section }

    /// Cuts the flat rows at their headers.
    nonisolated static func split(_ rows: [ChangeRow]) -> [SectionRows] {
        var out: [SectionRows] = []
        var current: (ChangeSection, [String])?
        var items: [ChangeRow] = []
        for row in rows {
            if case .header(let s, let paths) = row {
                if let (cs, cp) = current { out.append(SectionRows(section: cs, paths: cp, rows: items)) }
                current = (s, paths)
                items = []
            } else {
                items.append(row)
            }
        }
        if let (cs, cp) = current { out.append(SectionRows(section: cs, paths: cp, rows: items)) }
        return out
    }

    /// The paths a selection of row ids picks in one section.
    nonisolated static func paths(_ selection: Set<String>, in section: ChangeSection) -> [String] {
        let prefix = section.rawValue + ":"
        return selection.filter { $0.hasPrefix(prefix) && !$0.hasSuffix("/") }.map { String($0.dropFirst(prefix.count)) }.sorted()
    }
}

/// A tree's changes: its branch, the commit composer, then Staged and Changes, flat or as a tree.
@MainActor
struct ChangesScreen: View {
    let host: HostModel
    let mac: String
    let tree: String
    @AppStorage("git.changes.tree") private var asTree = false
    @State private var folded: Set<String> = []
    @State private var selection: Set<String> = []
    @State private var discarding: [String]?

    var body: some View {
        let store = host.git
        let changes = store.changes(tree)
        let sections = SectionRows.split(ChangeTree.rows(changes.files, tree: asTree, folded: folded))
        List(selection: $selection) {
            Section {
                if let head = changes.head {
                    HeadRow(store: store, changes: changes, head: head, mac: mac)
                } else if changes.loading {
                    HStack { ProgressView(); Text("Reading changes…").foregroundStyle(Palette.text3.color) }
                }
                if let error = changes.error {
                    ErrorRow(message: error) { changes.error = nil }
                }
            }
            if changes.head != nil {
                Section { CommitComposer(store: store, changes: changes) }
            }
            if changes.head != nil, changes.files.isEmpty {
                Section {
                    Label("No changes", systemImage: "checkmark.circle").foregroundStyle(Palette.text3.color)
                }
            }
            ForEach(sections) { s in
                Section {
                    ForEach(s.rows) { row in rowView(row, store: store) }
                } header: {
                    SectionHeader(
                        section: s.section, count: s.paths.count, folded: folded.contains(s.section.rawValue),
                        fold: { ChangeTree.toggleFold(&folded, s.section.rawValue) },
                        stageAll: { stage(s.paths, staged: s.section == .changes) })
                }
            }
            if changes.nextCursor != nil {
                HStack {
                    Spacer()
                    ProgressView()
                    Spacer()
                }
                .onAppear { Task { await store.loadMore(tree) } }
            }
        }
        #if os(iOS)
            .listStyle(.insetGrouped)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) { EditButton() }
                ToolbarItemGroup(placement: .bottomBar) {
                    if !selection.isEmpty { selectionBar }
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
                selection = []
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

    @ViewBuilder
    private func rowView(_ row: ChangeRow, store: GitStore) -> some View {
        switch row {
        case .header:
            EmptyView()
        case .folder(_, let key, let label, let depth, let open):
            Button {
                ChangeTree.toggleFold(&folded, key)
            } label: {
                FolderRow(label: label, depth: depth, open: open)
            }
            .buttonStyle(.plain)
        case .file(let section, let f, let depth):
            let staged = section == .staged
            NavigationLink(value: Route.diff(mac: mac, tree: tree, path: f.path, staged: staged)) {
                FileRow(
                    path: f.path, oldPath: f.oldPath, status: f.status, counts: GitText.counts(added: f.added, removed: f.removed, binary: f.binary == true),
                    depth: depth, nameOnly: asTree)
            }
            .tag(row.id)
            .swipeActions(edge: .leading, allowsFullSwipe: true) {
                Button(staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle") { stage([f.path], staged: !staged) }
                    .tint(staged ? Palette.text3.color : Palette.success.color)
            }
            .swipeActions(edge: .trailing) {
                if !staged {
                    Button("Discard", systemImage: "arrow.uturn.backward", role: .destructive) { discarding = [f.path] }
                }
            }
            .contextMenu {
                Button(staged ? "Unstage" : "Stage", systemImage: staged ? "minus.circle" : "plus.circle") { stage([f.path], staged: !staged) }
                if !staged {
                    Button("Discard Changes…", systemImage: "arrow.uturn.backward", role: .destructive) { discarding = [f.path] }
                }
                Button("Copy Path", systemImage: "doc.on.doc") { Pasteboard.copy(f.path) }
            }
        }
    }

    #if os(iOS)
        @ViewBuilder
        private var selectionBar: some View {
            let changed = SectionRows.paths(selection, in: .changes)
            let staged = SectionRows.paths(selection, in: .staged)
            Button("Stage \(changed.count)") { stage(changed, staged: true) }.disabled(changed.isEmpty)
            Spacer()
            Button("Unstage \(staged.count)") { stage(staged, staged: false) }.disabled(staged.isEmpty)
            Spacer()
            Button("Discard", role: .destructive) { discarding = changed }.disabled(changed.isEmpty)
        }
    #endif

    private func stage(_ paths: [String], staged: Bool) {
        guard !paths.isEmpty else { return }
        Haptics.tap()
        selection = []
        Task { await host.git.stage(tree, paths: paths, staged: staged) }
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
