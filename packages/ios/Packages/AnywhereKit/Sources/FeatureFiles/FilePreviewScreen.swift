import CodeView
import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI
import UniformTypeIdentifiers
import Wire

/// One file: code, rendered markdown (or its source), an image to zoom, an SVG, or why it can't be shown. A path that
/// turns out to be a folder shows the tree there.
@MainActor
struct FilePreviewScreen: View {
    let files: FilesStore
    let preview: FilePreview
    /// The line (from 1) a link named, brought to the middle of the source.
    var line: Int?
    var context = FilesContext()

    @AppStorage("files.markdownSource") private var source = false
    @AppStorage("files.wrap") private var wrapChoice: Bool?
    @AppStorage("files.lineNumbers") private var lineNumbers = true
    @AppStorage("files.textSize") private var textSize = 0
    /// Source or rendered on this screen only; until picked, a link to a line or Find shows the source.
    @State private var sourceHere: Bool?
    @State private var linked: FilePick?
    @State private var selecting = false
    @State private var goingTo = false
    @State private var finding = false
    @State private var handoff: Handoff?
    @State private var toast: Toast?
    @Environment(\.openURL) private var openURL
    #if os(iOS)
        @Environment(\.horizontalSizeClass) private var sizeClass
    #endif

    var body: some View {
        let _ = BodyCount.hit("FilePreviewScreen")
        if case .folder = preview.phase {
            FileTreeScreen(files: files, model: files.tree(preview.tree), dir: preview.path, context: context)
        } else {
            file
        }
    }

    private var file: some View {
        let read = loaded
        let shown = PreviewBody.of(preview.phase, path: preview.path, source: showsSource)
        return VStack(spacing: 0) {
            if let read, let note = PreviewBody.truncation(read) {
                strip(note, systemImage: "scissors")
            }
            if let notice = preview.notice {
                strip(notice, systemImage: "exclamationmark.triangle")
            }
            content(shown)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .screenBackground()
        .fileTitle(FileText.baseName(preview.path), subtitle: PreviewBody.subtitle(read, path: preview.path))
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar { toolbar(read, shown) }
        .task(id: preview.path) { files.load(preview) }
        .navigationDestination(item: $linked) { pick in
            FilePreviewScreen(files: files, preview: files.preview(tree: preview.tree, path: pick.path), line: pick.line, context: context)
        }
        .sheet(isPresented: $goingTo) {
            GoToFileSheet(search: files.makeSearch(tree: preview.tree)) { path in
                goingTo = false
                linked = FilePick(path: path)
            }
        }
        #if os(iOS)
            .sheet(isPresented: $selecting) { SelectTextSheet(loaded?.text ?? "", monospaced: true) }
        #endif
        .handoff($handoff, files: files, tree: preview.tree, context: context, toast: $toast)
        .toast($toast)
        .watching(preview.tree, context)
    }

    private var loaded: FsRead? {
        if case .loaded(let read) = preview.phase { read } else { nil }
    }

    private var showsSource: Bool { sourceHere ?? (source || line != nil) }

    private var wrap: Bool {
        #if os(iOS)
            PreviewSettings.wrap(wrapChoice, compact: sizeClass == .compact)
        #else
            PreviewSettings.wrap(wrapChoice, compact: false)
        #endif
    }

    private func strip(_ text: String, systemImage: String) -> some View {
        Label(text, systemImage: systemImage)
            .font(Typography.footnote)
            .foregroundStyle(Palette.text3.color)
            .frame(maxWidth: .infinity)
            .padding(.vertical, Space.xs)
            .background(Palette.fill4.color)
    }

    @ViewBuilder
    private func content(_ shown: PreviewBody) -> some View {
        switch shown {
        case .loading, .folder:
            ProgressView()
        case .code(let text, let language):
            CodeTextView(
                text: text, language: language, lineNumbers: lineNumbers, wrap: wrap, textSize: textSize, line: line, finding: $finding,
                onAsk: context.canAsk ? { handoff = .quote($0.payload(path: preview.path)) } : nil)
        case .markdown(let text):
            ScrollView {
                MarkdownView(text, style: .document, onLink: open)
                    .padding(Space.l)
                    .frame(maxWidth: 760, alignment: .leading)
                    .frame(maxWidth: .infinity)
            }
            .environment(\.repoImages, repoImages)
        case .svg(let text):
            SVGView(source: text)
        case .image(let data):
            ZoomableImage(data: data, name: FileText.baseName(preview.path))
        case .failed(let message):
            Placeholder(message, systemImage: "exclamationmark.triangle") { files.load(preview) }
        case .note(let text, let systemImage, let detail):
            Placeholder(text, systemImage: systemImage, detail: detail ?? preview.path)
        }
    }

    /// The images the markdown names, read from beside it in the tree.
    private var repoImages: RepoImages {
        let (files, tree, file) = (files, preview.tree, preview.path)
        return RepoImages(scope: tree + "/" + file) { source in
            guard let path = FileText.linked(source.removingPercentEncoding ?? source, from: file, tree: tree) else { return nil }
            return await files.image(tree: tree, path: path)
        }
    }

    private func open(_ link: MarkdownLink) {
        switch link {
        case .web(let url): openURL(url)
        case .file(let path, let line):
            if let path = FileText.linked(path, from: preview.path, tree: preview.tree) { linked = FilePick(path: path, line: line) }
        }
    }

    /// Find searches the source, so rendered markdown or SVG shows its source for it.
    private func find(_ shown: PreviewBody) {
        if case .code = shown {} else { sourceHere = true }
        finding = true
    }

    @ToolbarContentBuilder
    private func toolbar(_ read: FsRead?, _ shown: PreviewBody) -> some ToolbarContent {
        if PreviewBody.hasSource(read, path: preview.path) {
            #if os(iOS)
                ToolbarItem(placement: .bottomBar) { sourcePicker }
            #else
                ToolbarItem(placement: .automatic) { sourcePicker }
            #endif
        }
        if read?.kind == .text {
            ToolbarItem(placement: .primaryAction) {
                Button { find(shown) } label: { Label("Find", systemImage: "magnifyingglass") }
                    .keyboardShortcut("f", modifiers: .command)
            }
        }
        ToolbarItem(placement: .primaryAction) { more(read, shown) }
    }

    private var sourcePicker: some View {
        Picker("Show", selection: Binding(get: { showsSource }, set: { shown in sourceHere = shown; source = shown })) {
            Text("Preview").tag(false)
            Text("Source").tag(true)
        }
        .pickerStyle(.segmented)
        .fixedSize()
    }

    private func more(_ read: FsRead?, _ shown: PreviewBody) -> some View {
        Menu {
            if context.target != .none {
                Button { handoff = .mention(path: preview.path) } label: { Label("Mention in Chat", systemImage: "at") }
            }
            Button { copy(FileText.absolute(tree: preview.tree, preview.path)) } label: { Label("Copy Path", systemImage: "doc.on.doc") }
            #if os(iOS)
                if read?.text != nil {
                    Button { selecting = true } label: { Label("Select Text", systemImage: "text.cursor") }
                }
            #endif
            Button { goingTo = true } label: { Label("Go to File", systemImage: "doc.text.magnifyingglass") }
            if case .code = shown { settings }
            if let read { share(read) }
            Button { files.load(preview) } label: { Label("Reload", systemImage: "arrow.clockwise") }
        } label: {
            Label("More", systemImage: "ellipsis.circle")
        }
    }

    /// The desktop's Files settings for code: wrap, line numbers, text size.
    private var settings: some View {
        Section {
            Toggle(isOn: Binding(get: { wrap }, set: { wrapChoice = $0 })) { Label("Wrap Lines", systemImage: "text.word.spacing") }
            Toggle(isOn: $lineNumbers) { Label("Line Numbers", systemImage: "list.number") }
            Menu {
                Button { textSize = PreviewSettings.step(textSize, by: -1) ?? textSize } label: {
                    Label("Smaller", systemImage: "textformat.size.smaller")
                }
                .disabled(PreviewSettings.step(textSize, by: -1) == nil)
                Button { textSize = PreviewSettings.step(textSize, by: 1) ?? textSize } label: {
                    Label("Larger", systemImage: "textformat.size.larger")
                }
                .disabled(PreviewSettings.step(textSize, by: 1) == nil)
            } label: {
                Label("Text Size", systemImage: "textformat.size")
            }
        }
    }

    private func copy(_ path: String) {
        Clipboard.copy(path)
        Haptics.success()
        toast = Toast("Copied path")
    }

    @ViewBuilder
    private func share(_ read: FsRead) -> some View {
        let name = FileText.baseName(preview.path)
        if let text = read.text {
            ShareLink(item: text, subject: Text(name), preview: SharePreview(name)) { Label("Share", systemImage: "square.and.arrow.up") }
        } else if let data = read.data {
            ShareLink(item: SharedImage(data: data, name: name), preview: SharePreview(name)) {
                Label("Share", systemImage: "square.and.arrow.up")
            }
        }
    }
}

/// An image shared as its own bytes, written to a file only once the reader shares it.
struct SharedImage: Transferable {
    let data: Data
    let name: String

    static var transferRepresentation: some TransferRepresentation {
        FileRepresentation(exportedContentType: .image) { image in
            let dir = FileManager.default.temporaryDirectory.appending(path: UUID().uuidString)
            try FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
            let url = dir.appending(path: image.name)
            try image.data.write(to: url)
            return SentTransferredFile(url)
        }
    }
}
