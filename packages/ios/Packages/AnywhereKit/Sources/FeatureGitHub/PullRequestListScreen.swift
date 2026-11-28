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
    var macName = ErrorText.unnamedMac
    let project: String
    var load = true

    @State private var error: String?

    var body: some View {
        let items = store.lists[project]
        let problem = store.listProblems[project]
        Group {
            if let items, !items.isEmpty {
                List {
                    if let problem {
                        Section { problemRow(problem) }
                    }
                    ForEach(items, id: \.tree) { item in
                        NavigationLink(value: Route.pullRequest(mac: mac, tree: item.tree)) {
                            PrRow(entry: PrEntry(mac: mac, macName: macName, projectPath: project, repo: (project as NSString).lastPathComponent, item: item),
                                  act: { _ in })
                        }
                        .listRowInsets(EdgeInsets())
                    }
                }
                .groupedList()
                .refreshable { await reload() }
            } else if let problem {
                Pullable(refresh: reload) { GhProblemView(problem: problem, mac: macName) { Task { await reload() } } }
            } else if items != nil {
                Pullable(refresh: reload) {
                    Placeholder("No pull requests", icon: .pullRequest, detail: "No worktree of this project has an open pull request.")
                }
            } else if let error {
                Pullable(refresh: reload) {
                    Placeholder("Couldn't load pull requests", icon: .triangleAlert, detail: error) { Task { await reload() } }
                }
            } else {
                ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity).screenBackground()
            }
        }
        .navigationTitle("Pull requests")
        .task(id: project) { if load { await reload() } }
    }

    private func problemRow(_ problem: GhState) -> some View {
        let (title, detail) = GhProblemView.text(problem, mac: macName)
        return HStack(spacing: Space.m) {
            VStack(alignment: .leading, spacing: Space.xxs) {
                Text(title).font(Typography.subheadline.weight(.semibold)).foregroundStyle(Palette.waitingText.color)
                Text("\(detail) Until then this list isn't kept fresh.").font(Typography.footnote).foregroundStyle(Palette.text3.color)
            }
            Spacer(minLength: 0)
            Button { Task { await reload() } } label: { Text("Check Again").hitTarget() }.buttonStyle(.borderless)
        }
    }

    private func reload() async {
        do {
            try await store.recheckList(project)
            error = nil
        } catch {
            self.error = store.describe(error)
        }
    }
}
