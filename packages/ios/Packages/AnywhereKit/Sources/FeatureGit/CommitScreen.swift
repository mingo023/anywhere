import DesignSystem
import Foundation
import Stores
import SwiftUI
import Wire

/// A file of a commit, opened as that commit's diff.
struct CommitFileRoute: Hashable {
    let tree: String
    let sha: String
    let path: String
    let siblings: [String]
}

/// One commit: its message, author and time, and the files it changed.
@MainActor
struct CommitScreen: View {
    let host: HostModel
    let tree: String
    let sha: String
    @State private var detail: CommitDetail?
    @State private var error: String?

    init(host: HostModel, tree: String, sha: String) {
        self.host = host
        self.tree = tree
        self.sha = sha
        _detail = State(initialValue: host.git.cachedCommit(sha))
    }

    var body: some View {
        Group {
            if let detail {
                List {
                    Section {
                        VStack(alignment: .leading, spacing: Space.s) {
                            Text(detail.subject).font(Typography.headline).foregroundStyle(Palette.text.color)
                            if let body = detail.body {
                                Text(body).font(Typography.subheadline).foregroundStyle(Palette.textBody.color).textSelection(.enabled)
                            }
                            HStack(spacing: Space.xs) {
                                Image(systemName: "person.crop.circle")
                                Text(detail.author)
                                Text("·")
                                Text(Date(timeIntervalSince1970: TimeInterval(detail.at)), format: .dateTime.day().month().year().hour().minute())
                            }
                            .font(Typography.caption)
                            .foregroundStyle(Palette.text3.color)
                            Button {
                                Pasteboard.copy(detail.sha)
                                Haptics.success()
                            } label: {
                                Label(GitText.short(detail.sha), systemImage: "doc.on.doc").font(Typography.codeSmall)
                            }
                            .buttonStyle(.borderless)
                            .accessibilityLabel("Copy commit \(GitText.short(detail.sha))")
                        }
                        .padding(.vertical, Space.xs)
                    }
                    Section("\(detail.files.count) file\(detail.files.count == 1 ? "" : "s")") {
                        let paths = detail.files.map(\.path)
                        ForEach(detail.files, id: \.path) { f in
                            NavigationLink(value: CommitFileRoute(tree: tree, sha: detail.sha, path: f.path, siblings: paths)) {
                                FileRow(
                                    path: f.path, oldPath: f.oldPath, status: f.status,
                                    counts: GitText.counts(added: f.added, removed: f.removed, binary: f.added == nil && f.status != .deleted))
                            }
                        }
                    }
                }
                #if os(iOS)
                    .listStyle(.insetGrouped)
                #endif
            } else if let error {
                Placeholder("Couldn't load the commit", systemImage: "exclamationmark.triangle", detail: error)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .navigationTitle(GitText.short(sha))
        .navigationDestination(for: CommitFileRoute.self) { r in
            DiffScreen(host: host, tree: r.tree, path: r.path, at: .commit(rev: r.sha), siblings: r.siblings)
        }
        .task {
            guard detail == nil else { return }
            do {
                detail = try await host.git.commit(tree, sha: sha)
            } catch {
                self.error = GitStore.describe(error)
            }
        }
    }
}
