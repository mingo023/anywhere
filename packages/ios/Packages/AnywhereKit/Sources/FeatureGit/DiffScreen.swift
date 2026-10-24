import CodeView
import DesignSystem
import DiffModel
import Stores
import SwiftUI
import Wire

/// One file's diff with its header, next and previous file, and side by side on a wide screen.
@MainActor
struct DiffScreen: View {
    let host: HostModel
    let tree: String
    let at: DiffAt
    /// The files Next and Previous step through, in list order.
    let siblings: [String]
    @State private var path: String
    @State private var doc: DiffDocument?
    @State private var error: String?
    @State private var ignoreWhitespace = false
    @AppStorage("git.diff.sideBySide") private var sideBySide = true

    init(host: HostModel, tree: String, path: String, at: DiffAt, siblings: [String]) {
        self.host = host
        self.tree = tree
        self.at = at
        self.siblings = siblings
        _path = State(initialValue: path)
        _doc = State(initialValue: host.git.cachedDiff(tree, path: path, at: at))
    }

    var body: some View {
        let rev = host.git.changes(tree).head?.rev
        GeometryReader { geo in
            VStack(spacing: 0) {
                DiffHeader(path: path, doc: doc, at: at)
                Divider()
                content(wide: geo.size.width >= 700)
            }
        }
        .screenBackground()
        .navigationTitle(GitText.split(path).name)
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItemGroup(placement: .primaryAction) {
                Button("Previous File", systemImage: "chevron.up") { step(-1) }.disabled(index.map { $0 == 0 } ?? true)
                Button("Next File", systemImage: "chevron.down") { step(1) }.disabled(index.map { $0 + 1 >= siblings.count } ?? true)
                Menu {
                    Toggle("Side by Side When Wide", systemImage: "rectangle.split.2x1", isOn: $sideBySide)
                    Toggle("Ignore Whitespace", systemImage: "space", isOn: $ignoreWhitespace)
                } label: {
                    Label("Options", systemImage: "ellipsis.circle")
                }
            }
        }
        .task(id: Load(path: path, ignoreWhitespace: ignoreWhitespace, rev: rev)) {
            do {
                error = nil
                doc = try await host.git.diff(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace)
            } catch {
                if doc == nil { self.error = GitStore.describe(error) }
            }
        }
    }

    private struct Load: Equatable {
        let path: String
        let ignoreWhitespace: Bool
        let rev: String?
    }

    private var index: Int? { siblings.firstIndex(of: path) }

    private func step(_ by: Int) {
        guard let i = index, siblings.indices.contains(i + by) else { return }
        Haptics.tap()
        path = siblings[i + by]
        doc = host.git.cachedDiff(tree, path: path, at: at, ignoreWhitespace: ignoreWhitespace)
    }

    @ViewBuilder
    private func content(wide: Bool) -> some View {
        if let doc, doc.path == path {
            switch doc.state {
            case .rows:
                VStack(spacing: 0) {
                    DiffView(doc, sideBySide: wide && sideBySide)
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
