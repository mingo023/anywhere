import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A project's PRs, one per worktree that has one, as the Mac last heard from gh.
@MainActor
struct PullRequestListScreen: View {
    let store: GitHubStore
    let mac: String
    let project: String
    var load = true

    @State private var error: String?

    var body: some View {
        let items = store.lists[project]
        Group {
            if let items, !items.isEmpty {
                List(items, id: \.tree) { item in
                    NavigationLink(value: Route.pullRequest(mac: mac, tree: item.tree)) { PullRequestRow(item: item) }
                }
                .groupedList()
            } else if items != nil {
                Placeholder("No pull requests", systemImage: "arrow.triangle.pull", detail: "No worktree of this project has an open pull request.")
            } else if let error {
                Placeholder("Couldn't load pull requests", systemImage: "exclamationmark.triangle", detail: error)
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
            }
        }
        .navigationTitle("Pull requests")
        .refreshable { await reload() }
        .task(id: project) { if load { await reload() } }
    }

    private func reload() async {
        do {
            try await store.refreshList(project)
            error = nil
        } catch {
            self.error = GitHubStore.describe(error)
        }
    }
}

/// One PR in a list: its state, number and title, and its checks in the colour of the worst.
@MainActor
struct PullRequestRow: View, Equatable {
    let item: PrItem

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: Space.m) {
            PrGlyph(state: item.state, draft: item.draft)
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(item.title).font(Typography.body).foregroundStyle(Palette.text.color).lineLimit(2)
                HStack(spacing: Space.xs) {
                    Text("#\(item.number) · \((item.tree as NSString).lastPathComponent)").foregroundStyle(Palette.text3.color)
                    Text("·").foregroundStyle(Palette.text4.color)
                    let tone = PullRequestText.tone(state: item.state, draft: item.draft, checks: item.checks)
                    Text(PullRequestText.detail(state: item.state, draft: item.draft, checks: item.checks)).foregroundStyle(tone.text.color)
                }
                .font(Typography.footnote)
                .lineLimit(1)
            }
        }
        .padding(.vertical, Space.xxs)
        .accessibilityElement(children: .combine)
    }
}
