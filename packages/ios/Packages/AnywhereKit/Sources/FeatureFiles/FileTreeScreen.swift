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
            .navigationTitle(FileText.baseName(model.tree))
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
            Placeholder("Can't show these files", systemImage: "folder.badge.questionmark", detail: failure) { files.open(model) }
        } else if !model.loaded {
            ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
        } else if model.rows.isEmpty {
            Placeholder("No files", systemImage: "folder", detail: hideIgnored || hideDotfiles ? "Hidden files aren't shown." : nil)
        } else {
            list
        }
    }

    /// The folder the route named is listed, so its row can be scrolled to.
    private var dirListed: Bool { !dir.isEmpty && model.state.listings[FileText.folder(dir)] != nil }

    private var list: some View {
        ScrollViewReader { proxy in
            List(model.rows) { row in
                Button { open(row) } label: { FileRowView(row: row).equatable().contentShape(Rectangle()) }
                    .buttonStyle(.plain)
                    .overlay(alignment: .trailing) {
                        if row.failure != nil {
                            Button { files.retry(model, row.path) } label: {
                                Image(systemName: "arrow.clockwise").foregroundStyle(Palette.accent.color).hitTarget()
                            }
                            .buttonStyle(.borderless)
                            .accessibilityLabel("Try Again")
                        }
                    }
                    .listRowInsets(EdgeInsets(top: 0, leading: Space.l, bottom: 0, trailing: Space.l))
                    .listRowBackground(Palette.background.color)
                    .swipeActions(edge: .trailing) {
                        if context.target != .none {
                            Button { handoff = .mention(path: row.path) } label: { Label("Mention", systemImage: "at") }.tint(Palette.accent.color)
                        }
                        Button { copy(FileText.absolute(tree: model.tree, row.path)) } label: { Label("Copy Path", systemImage: "doc.on.doc") }
                            .tint(.gray)
                    }
                    .contextMenu { menu(row) }
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .screenBackground()
            .refreshable { files.open(model) }
            .task(id: dirListed) { if dirListed { proxy.scrollTo(dir, anchor: .top) } }
        }
    }

    @ViewBuilder
    private func menu(_ row: FileRow) -> some View {
        if !row.entry.dir {
            Button { picked = FilePick(path: row.path) } label: { Label("Open", systemImage: "doc.text") }
        }
        if context.target != .none {
            Button { handoff = .mention(path: row.path) } label: { Label("Mention in Chat", systemImage: "at") }
        }
        Button { copy(FileText.absolute(tree: model.tree, row.path)) } label: { Label("Copy Path", systemImage: "doc.on.doc") }
        Button { copy(row.path) } label: { Label("Copy Relative Path", systemImage: "arrow.turn.down.right") }
        if let letter = row.entry.git {
            Text(GitLetter.word(letter))
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .primaryAction) {
            Button { searching = true } label: { Label("Go to File", systemImage: "magnifyingglass") }
                .keyboardShortcut("p", modifiers: .command)
        }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                Toggle(isOn: Binding(get: { !hideIgnored }, set: { hideIgnored = !$0 })) { Label("Show Ignored Files", systemImage: "eye") }
                Toggle(isOn: Binding(get: { !hideDotfiles }, set: { hideDotfiles = !$0 })) { Label("Show Dotfiles", systemImage: "circle.dotted") }
                Divider()
                Button { copy(model.tree) } label: { Label("Copy Folder Path", systemImage: "doc.on.doc") }
                Button { files.open(model) } label: { Label("Refresh", systemImage: "arrow.clockwise") }
            } label: {
                Label("Options", systemImage: "ellipsis.circle")
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

/// One line of the tree: indent, chevron or icon, name, git letter; a folder that couldn't be listed says why.
@MainActor
struct FileRowView: View, Equatable {
    let row: FileRow

    nonisolated static let indent: CGFloat = 16

    var body: some View {
        HStack(spacing: Space.s) {
            Group {
                if row.entry.dir {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 11, weight: .semibold))
                        .foregroundStyle(Palette.text4.color)
                        .rotationEffect(.degrees(row.open ? 90 : 0))
                } else {
                    Color.clear
                }
            }
            .frame(width: 12)
            Image(systemName: Self.icon(row))
                .font(.system(size: 15))
                .symbolRenderingMode(.hierarchical)
                .foregroundStyle((row.entry.dir ? Palette.accent : Palette.text3).color)
                .frame(width: 20)
            Text(row.entry.name)
                .font(Typography.body)
                .foregroundStyle((row.entry.ignored == true ? Palette.text4 : Palette.text).color)
                .lineLimit(1)
                .truncationMode(.middle)
            if row.entry.link == true {
                Image(systemName: "arrow.up.forward").font(.caption2).foregroundStyle(Palette.text4.color)
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
                Text(letter)
                    .font(.system(.caption, design: .monospaced, weight: .bold))
                    .foregroundStyle(GitLetter.color(letter).color)
                    .accessibilityLabel(GitLetter.word(letter))
            }
        }
        .padding(.leading, CGFloat(row.depth) * Self.indent)
        // Room for the tree screen's Try Again over the row's end.
        .padding(.trailing, row.failure == nil ? 0 : HitTarget.minimum)
        .frame(minHeight: HitTarget.minimum)
        .accessibilityElement(children: .combine)
    }

    nonisolated static func icon(_ row: FileRow) -> String {
        if row.entry.dir { return row.open ? "folder.fill" : "folder" }
        switch FileText.ext(row.entry.name).lowercased() {
        case "png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "heic", "svg": return "photo"
        case "md", "markdown", "txt": return "doc.text"
        case "json", "toml", "yml", "yaml": return "curlybraces"
        case "zip", "gz", "tar": return "doc.zipper"
        case "": return "doc"
        default: return "chevron.left.forwardslash.chevron.right"
        }
    }
}
