import DesignSystem
import Domain
import Stores
import SwiftUI

/// A tree's folders and files, opened one folder at a time, with git's letters, as the desktop's Explorer.
@MainActor
struct FileTreeScreen: View {
    let files: FilesStore
    let model: FileTreeModel
    /// The folder the route opened at.
    var dir = ""
    var context = FilesContext()

    @AppStorage("files.hideIgnored") private var hideIgnored = true
    @AppStorage("files.hideDotfiles") private var hideDotfiles = false
    @State private var searching = false
    @State private var picked: FilePick?
    /// The route's folder is opened once per screen, so coming back keeps what the reader folded.
    @State private var revealed = false
    @State private var handoff: Handoff?
    @State private var toast: Toast?

    var body: some View {
        content
            .navigationTitle("Files")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar { toolbar }
            .sheet(isPresented: $searching) {
                GoToFileSheet(search: files.makeSearch(tree: model.tree)) { path in
                    searching = false
                    picked = FilePick(path: path)
                }
            }
            .navigationDestination(item: $picked) { pick in
                FilePreviewScreen(files: files, preview: files.preview(tree: model.tree, path: pick.path), context: context)
            }
            .task {
                files.filter = FileFilter(hideIgnored: hideIgnored, hideDotfiles: hideDotfiles)
                if !revealed {
                    revealed = true
                    files.reveal(model, dir)
                }
                files.open(model)
            }
            .onChange(of: FileFilter(hideIgnored: hideIgnored, hideDotfiles: hideDotfiles)) { _, filter in files.filter = filter }
            .handoff($handoff, files: files, tree: model.tree, context: context, toast: $toast)
            .toast($toast)
            .watching(model.tree, context)
    }

    @ViewBuilder
    private var content: some View {
        if let failure = model.failure, !model.loaded {
            FilesNote(title: "Can't show these files", detail: context.offline?.note() ?? failure) { files.open(model) }
        } else if !model.loaded {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
        } else if model.rows.isEmpty {
            Placeholder("No files", icon: .folder, detail: hideIgnored || hideDotfiles ? "Hidden files aren't shown." : nil)
        } else {
            list
        }
    }

    /// The folder the route named is listed, so its row can be scrolled to.
    private var dirListed: Bool { !dir.isEmpty && model.state.listings[FileText.folder(dir)] != nil }

    private var list: some View {
        let rows = model.rows
        let (first, last) = (rows.first?.id, rows.last?.id)
        return ScrollViewReader { proxy in
            List {
                Button { searching = true } label: {
                    SearchBox { Text("Go to file").font(Typography.body).foregroundStyle(Palette.text3.color).frame(maxWidth: .infinity, alignment: .leading) }
                }
                .buttonStyle(.plain)
                .keyboardShortcut("p", modifiers: .command)
                .listRowInsets(EdgeInsets(top: Space.xs, leading: Space.l, bottom: Space.l, trailing: Space.l))
                .listRowBackground(Palette.background.color)
                .listRowSeparator(.hidden)
                ForEach(rows) { row in
                    Button { open(row) } label: { FileRowView(row: row, first: row.id == first).equatable().contentShape(Rectangle()) }
                        .buttonStyle(.plain)
                        .overlay(alignment: .trailing) {
                            if row.failure != nil {
                                Button { files.retry(model, row.path) } label: {
                                    LucideIcon(.rotateCw, size: 17).foregroundStyle(Palette.accent.color).hitTarget()
                                }
                                .buttonStyle(.borderless)
                                .accessibilityLabel("Try Again")
                            }
                        }
                        .listRowInsets(EdgeInsets(top: 0, leading: Inset.margin, bottom: 0, trailing: Inset.margin))
                        .listRowBackground(card(first: row.id == first, last: row.id == last))
                        .swipeActions(edge: .trailing) {
                            if context.target != .none {
                                Button { handoff = .mention(path: row.path) } label: { Label("Mention", lucide: .atSign) }.tint(Palette.accent.color)
                            }
                            Button { copy(FileText.absolute(tree: model.tree, row.path)) } label: { Label("Copy Path", lucide: .copy) }
                                .tint(.gray)
                        }
                        .contextMenu { menu(row) }
                }
                .listRowSeparator(.hidden)
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .screenBackground()
            .refreshable { files.open(model) }
            .task(id: dirListed) { if dirListed { proxy.scrollTo(dir, anchor: .top) } }
        }
    }

    /// A row of the tree's card: the card's surface, rounded at its first row's top and its last row's bottom.
    private func card(first: Bool, last: Bool) -> some View {
        let r = Inset.radius
        return UnevenRoundedRectangle(
            topLeadingRadius: first ? r : 0, bottomLeadingRadius: last ? r : 0, bottomTrailingRadius: last ? r : 0, topTrailingRadius: first ? r : 0,
            style: .continuous
        )
        .fill(Palette.surface.color)
        .padding(.horizontal, Inset.margin)
    }

    @ViewBuilder
    private func menu(_ row: FileRow) -> some View {
        if !row.entry.dir {
            Button { picked = FilePick(path: row.path) } label: { Label("Open", lucide: .fileText) }
        }
        if context.target != .none {
            Button { handoff = .mention(path: row.path) } label: { Label("Mention in Chat", lucide: .atSign) }
        }
        Button { copy(FileText.absolute(tree: model.tree, row.path)) } label: { Label("Copy Path", lucide: .copy) }
        Button { copy(row.path) } label: { Label("Copy Relative Path", lucide: .cornerDownRight) }
        if let letter = row.entry.git {
            Text(GitLetter.word(letter))
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .principal) { BarHeading(title: "Files", detail: FileText.baseName(model.tree)) }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Toggle(isOn: Binding(get: { !hideIgnored }, set: { hideIgnored = !$0 })) { Label("Show Ignored Files", lucide: .eye) }
                // No Lucide icon is a dotted circle.
                Toggle(isOn: Binding(get: { !hideDotfiles }, set: { hideDotfiles = !$0 })) { Label("Show Dotfiles", systemImage: "circle.dotted") }
                Divider()
                Button { copy(model.tree) } label: { Label("Copy Folder Path", lucide: .copy) }
                Button { files.open(model) } label: { Label("Refresh", lucide: .rotateCw) }
            } label: {
                BarMenuLabel()
            }
        }
    }

    private func open(_ row: FileRow) {
        if row.entry.dir {
            Haptics.tap()
            withAnimation(.snappy(duration: 0.2)) { files.toggle(model, row.path) }
        } else {
            picked = FilePick(path: row.path)
        }
    }

    private func copy(_ path: String) {
        Clipboard.copy(path)
        Haptics.success()
        toast = Toast("Copied path")
    }
}

/// A file picked to preview, pushed on the tree's stack; `line` (from 1) when a link named one.
struct FilePick: Hashable, Identifiable {
    let path: String
    var line: Int?
    var id: String { path }
}

/// One line of the tree: indent, chevron, icon, name, git letter; a folder that couldn't be listed says why. A line is
/// 44 pt, its hairline starting at the name.
@MainActor
struct FileRowView: View, Equatable {
    let row: FileRow
    /// The card's top row, which has no hairline above it.
    let first: Bool

    nonisolated static let indent: CGFloat = 22

    var body: some View {
        HStack(spacing: 6) {
            Group {
                if row.entry.dir {
                    LucideIcon(.chevronRight, size: 13)
                        .foregroundStyle(Palette.text4.color)
                        .rotationEffect(.degrees(row.open ? 90 : 0))
                } else {
                    Color.clear
                }
            }
            .frame(width: 14)
            LucideIcon(row.entry.dir ? .folder : .fileText, size: 17).foregroundStyle(Palette.text2.color)
            HStack(spacing: Space.s) {
                Text(row.entry.name)
                    .font(.system(size: 16))
                    .foregroundStyle((row.entry.ignored == true ? Palette.text4 : Palette.text).color)
                    .lineLimit(1)
                    .truncationMode(.middle)
                if row.entry.link == true {
                    LucideIcon(.external, size: 11).foregroundStyle(Palette.text4.color)
                }
                Spacer(minLength: Space.s)
                if row.loading {
                    ProgressView().controlSize(.mini)
                }
                if let failure = row.failure {
                    Text(failure)
                        .font(Typography.footnote)
                        .foregroundStyle(Palette.failed.color)
                        .lineLimit(1)
                }
                if let letter = row.entry.git {
                    Text(row.entry.dir ? "•" : letter)
                        .font(.system(size: 13, design: .monospaced))
                        .foregroundStyle(GitLetter.color(letter).color)
                        .accessibilityLabel(GitLetter.word(letter))
                }
            }
            .padding(.leading, 4)
            // Room for the tree screen's Try Again over the row's end.
            .padding(.trailing, row.failure == nil ? Space.l : HitTarget.minimum)
            .frame(maxHeight: .infinity)
            .overlay(alignment: .top) { if !first { Palette.separator.color.frame(height: 0.5) } }
        }
        .padding(.leading, 12 + CGFloat(row.depth) * Self.indent)
        .frame(height: HitTarget.minimum)
        .accessibilityElement(children: .combine)
    }
}
