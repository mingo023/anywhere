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

/// One commit: its message, author and time, and the files it changed, each a bar that opens its diff.
@MainActor
struct CommitScreen: View {
    let host: HostModel
    let tree: String
    let sha: String
    @State private var detail: CommitDetail?
    @State private var error: String?
    /// Files marked as read on this screen; the Mac keeps no such mark.
    @State private var viewed: Set<String> = []

    init(host: HostModel, tree: String, sha: String) {
        self.host = host
        self.tree = tree
        self.sha = sha
        _detail = State(initialValue: host.git.cachedCommit(sha))
    }

    var body: some View {
        Group {
            if let detail {
                ScrollView {
                    VStack(alignment: .leading, spacing: 0) {
                        header(detail)
                        let paths = detail.files.map(\.path)
                        files(detail)
                        LazyVStack(spacing: 0) {
                            ForEach(detail.files, id: \.path) { f in
                                HStack(spacing: 0) {
                                    NavigationLink(value: CommitFileRoute(tree: tree, sha: detail.sha, path: f.path, siblings: paths)) {
                                        FileHeaderLead(
                                            title: GitText.shortPath(f.path), open: false,
                                            added: f.added, removed: f.added.map { _ in f.removed ?? 0 })
                                    }
                                    .buttonStyle(.plain)
                                    FileHeaderTail(viewed: viewed.contains(f.path), toggleViewed: { toggleViewed(f.path) }) {
                                        Button { Pasteboard.copy(f.path) } label: { Label("Copy Path", lucide: .copy) }
                                    }
                                }
                                .fileHeaderBar()
                            }
                        }
                    }
                    .padding(.bottom, Space.xl)
                }
            } else if let error {
                Placeholder("Couldn't load the commit", icon: .triangleAlert, detail: error)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
            }
        }
        .screenBackground()
        .navigationTitle("Commit")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .principal) { BarHeading("Commit", detail: GitText.short(sha)) }
            ToolbarItem(placement: .primaryAction) {
                Menu {
                    Button {
                        Pasteboard.copy(detail?.sha ?? sha)
                        Haptics.success()
                    } label: {
                        Label("Copy Hash", lucide: .copy)
                    }
                } label: {
                    BarMenuLabel()
                }
            }
        }
        .navigationDestination(for: CommitFileRoute.self) { r in
            DiffScreen(host: host, tree: r.tree, path: r.path, at: .commit(rev: r.sha), files: .list(r.siblings))
        }
        .task {
            guard detail == nil else { return }
            do {
                detail = try await host.git.commit(tree, sha: sha)
            } catch {
                self.error = ErrorText.describe(error, mac: host.name)
            }
        }
    }

    private func header(_ detail: CommitDetail) -> some View {
        let now = Int(Date().timeIntervalSince1970)
        return VStack(alignment: .leading, spacing: 0) {
            Text(detail.subject).font(.system(size: 20, weight: .bold)).foregroundStyle(Palette.text.color)
            if let body = detail.body {
                Text(body).font(.system(size: 15)).foregroundStyle(Palette.textBody.color).padding(.top, Space.s).textSelection(.enabled)
            }
            HStack(spacing: Space.s) {
                Text(GitText.initials(detail.author))
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(Palette.textBody.color)
                    .frame(width: 20, height: 20)
                    .background(Palette.fill3.color, in: Circle())
                Text(detail.author).fontWeight(.semibold).foregroundStyle(Palette.text.color).lineLimit(1)
                Text("committed \(GitText.committed(at: detail.at, now: now))").lineLimit(1)
                Spacer(minLength: Space.s)
                Button {
                    Pasteboard.copy(detail.sha)
                    Haptics.success()
                } label: {
                    Text(GitText.short(detail.sha))
                        .font(.system(size: 12, design: .monospaced))
                        .foregroundStyle(Palette.textBody.color)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(Palette.fill3.color, in: RoundedRectangle(cornerRadius: Radius.chip))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Copy commit \(GitText.short(detail.sha))")
            }
            .font(Typography.footnote)
            .foregroundStyle(Palette.text2.color)
            .padding(.top, Space.m)
        }
        .padding(.horizontal, Inset.margin)
        .padding(.top, Space.xs)
        .padding(.bottom, 18)
    }

    private func files(_ detail: CommitDetail) -> some View {
        HStack {
            Text("\(detail.files.count) file\(detail.files.count == 1 ? "" : "s") changed")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(Palette.text.color)
            Spacer()
            HStack(spacing: Space.xs) {
                Text("+\(detail.files.reduce(0) { $0 + ($1.added ?? 0) })").foregroundStyle(Palette.successText.color)
                Text("−\(detail.files.reduce(0) { $0 + ($1.removed ?? 0) })").foregroundStyle(Palette.failedText.color)
            }
            .font(Typography.codeSmall)
        }
        .padding(.horizontal, Inset.margin)
        .padding(.bottom, 10)
    }

    private func toggleViewed(_ path: String) {
        Haptics.tap()
        if viewed.remove(path) == nil { viewed.insert(path) }
    }
}
