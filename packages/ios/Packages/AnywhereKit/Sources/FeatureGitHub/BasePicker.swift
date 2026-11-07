import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The branch a new PR opens into, from the tree's branches as the New Session branch menu lists them; origin's
/// copy of a branch opens into that branch.
@MainActor
struct BasePicker: View {
    let branches: (@MainActor () async throws -> BranchList)?
    let mac: String
    @Binding var selection: String?

    @State private var list: BranchList?
    @State private var error: String?
    @State private var search = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let listed = WorktreeName.listed(local: list?.local.map(\.name) ?? [], remote: list?.remote.map(\.name) ?? [], search: search)
        List {
            if search.isEmpty {
                row(nil, label: "Default", detail: "The repository's default branch")
            }
            ForEach(listed, id: \.name) { branch in
                row(PullRequestText.baseBranch(branch.name), label: branch.name, detail: branch.local ? nil : "On origin")
            }
        }
        .overlay {
            if let error {
                Placeholder("Couldn't load branches", systemImage: "exclamationmark.triangle", detail: error) { Task { await load() } }
            } else if list == nil && branches != nil {
                ProgressView()
            } else if listed.isEmpty && !search.isEmpty {
                ContentUnavailableView.search(text: search)
            }
        }
        .searchable(text: $search, prompt: "Branches")
        .navigationTitle("Base")
        .task { await load() }
    }

    private func load() async {
        guard let branches else { return }
        error = nil
        do { list = try await branches() } catch { self.error = GitHubStore.describe(error, mac: mac) }
    }

    private func row(_ value: String?, label: String, detail: String?) -> some View {
        BranchRow(label, detail: detail, picked: selection == value) {
            selection = value
            dismiss()
        }
    }
}
