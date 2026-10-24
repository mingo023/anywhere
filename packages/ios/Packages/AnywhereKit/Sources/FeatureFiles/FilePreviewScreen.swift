import CodeView
import DesignSystem
import Domain
import MarkdownView
import Stores
import SwiftUI
import UniformTypeIdentifiers
import Wire

/// One file: code, rendered markdown (or its source), an image to zoom, an SVG, or why it can't be shown.
@MainActor
struct FilePreviewScreen: View {
    let files: FilesStore
    let preview: FilePreview

    @AppStorage("files.markdownSource") private var source = false

    var body: some View {
        let read = loaded
        VStack(spacing: 0) {
            if let read, let note = PreviewBody.truncation(read) {
                Label(note, systemImage: "scissors")
                    .font(Typography.footnote)
                    .foregroundStyle(Palette.text3.color)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, Space.xs)
                    .background(Palette.fill4.color)
            }
            content(PreviewBody.of(preview.phase, path: preview.path, source: source))
                .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .screenBackground()
        .navigationTitle(FileText.baseName(preview.path))
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar { toolbar(read) }
        .task(id: preview.path) { files.load(preview) }
    }

    private var loaded: FsRead? {
        if case .loaded(let read) = preview.phase { read } else { nil }
    }

    @ViewBuilder
    private func content(_ body: PreviewBody) -> some View {
        switch body {
        case .loading:
            ProgressView()
        case .code(let text, let language):
            CodeTextView(text: text, language: language)
        case .markdown(let text):
            ScrollView {
                MarkdownView(text, style: .document)
                    .padding(Space.l)
                    .frame(maxWidth: 760, alignment: .leading)
                    .frame(maxWidth: .infinity)
            }
        case .svg(let text):
            SVGView(source: text)
        case .image(let data):
            ZoomableImage(data: data, name: FileText.baseName(preview.path))
        case .note(let text, let systemImage):
            Placeholder(text, systemImage: systemImage, detail: preview.path)
        }
    }

    @ToolbarContentBuilder
    private func toolbar(_ read: FsRead?) -> some ToolbarContent {
        if PreviewBody.hasSource(read, path: preview.path) {
            ToolbarItem(placement: .principal) {
                Picker("Show", selection: $source) {
                    Text("Preview").tag(false)
                    Text("Source").tag(true)
                }
                .pickerStyle(.segmented)
                .fixedSize()
            }
        }
        ToolbarItem(placement: .primaryAction) {
            Menu {
                if let subtitle = PreviewBody.subtitle(read, path: preview.path) {
                    Text(subtitle)
                }
                Button {
                    files.mention(tree: preview.tree, path: preview.path)
                    Haptics.success()
                } label: { Label("Mention in Chat", systemImage: "at") }
                Button {
                    Clipboard.copy(FileText.absolute(tree: preview.tree, preview.path))
                    Haptics.success()
                } label: { Label("Copy Path", systemImage: "doc.on.doc") }
                if let read { share(read) }
                Button { files.load(preview) } label: { Label("Reload", systemImage: "arrow.clockwise") }
            } label: {
                Label("More", systemImage: "ellipsis.circle")
            }
        }
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
