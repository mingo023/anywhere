import DesignSystem
import Domain
import SwiftUI

/// A branch with no pull request yet: what one would open and its commits, then Create, or the way to the commit it lacks.
@MainActor
struct NoPullRequestView: View {
    let newPr: NewPr
    let mac: PrMac
    let tree: String
    let refresh: @MainActor () async -> Void
    let create: () -> Void

    var body: some View {
        List {
            header
            route
            if newPr.hasCommits, !newPr.commits.isEmpty { commits }
        }
        .scrollContentBackground(.hidden)
        .screenBackground()
        .groupedList()
        .refreshable { await refresh() }
        .safeAreaInset(edge: .bottom, spacing: 0) { action.disabled(mac.offline) }
    }

    private var header: some View {
        Section {
            VStack(alignment: .leading, spacing: Space.s) {
                PrPill(text: "No pull request", tone: .muted, dot: true)
                Text(newPr.hasCommits ? "Ready to open" : "Nothing to open yet").font(Typography.title).foregroundStyle(Palette.text.color)
                if !newPr.hasCommits {
                    Text("Commit your changes on this branch, then open a pull request.").font(Typography.subheadline).foregroundStyle(Palette.text3.color)
                }
            }
            .padding(.top, Space.xs)
            // Clears the section's rounded corner, which clips the start of a last line sitting lower.
            .padding(.bottom, Space.l)
        }
        .listRowBackground(Color.clear)
        .listRowInsets(EdgeInsets(top: 0, leading: Space.xs, bottom: 0, trailing: Space.xs))
        #if os(iOS)
            .listSectionSpacing(Space.s)
        #endif
    }

    private var route: some View {
        Section {
            fact("Branch", newPr.branch, systemImage: "branch")
            fact("Into", newPr.base.map(PullRequestText.baseBranch), systemImage: "arrow.triangle.merge")
        }
        .listRowBackground(Palette.surface.color)
    }

    private func fact(_ title: String, _ value: String?, systemImage: String) -> some View {
        LabeledContent {
            Text(value ?? "").font(Typography.codeSmall).lineLimit(1).truncationMode(.middle)
        } label: {
            Label { Text(title) } icon: { Image.named(systemImage, size: 17) }
        }
    }

    private var commits: some View {
        Section {
            ForEach(newPr.commits, id: \.sha) { c in
                NavigationLink(value: Route.commit(mac: mac.id, tree: tree, sha: c.sha)) {
                    HStack(spacing: Space.s) {
                        Text(String(c.sha.prefix(7))).font(Typography.codeSmall).foregroundStyle(Palette.text3.color)
                        Text(c.subject).lineLimit(1)
                    }
                }
            }
            if let ahead = newPr.ahead, ahead > newPr.commits.count {
                NavigationLink("Show all in History", value: Route.history(mac: mac.id, tree: tree))
            }
        } header: {
            Text(PullRequestText.commits(newPr.ahead ?? newPr.commits.count)).sectionTitle()
        }
        .listRowBackground(Palette.surface.color)
    }

    private var action: some View {
        Group {
            if newPr.hasCommits {
                Button(action: create) { Text("Create pull request").frame(maxWidth: .infinity) }
            } else {
                NavigationLink(value: Route.changes(mac: mac.id, tree: tree)) { Text("Go to Changes").frame(maxWidth: .infinity) }
            }
        }
        .buttonStyle(CapsuleButtonStyle())
        .padding(.horizontal, Space.l)
        .padding(.bottom, Space.xs)
    }
}
