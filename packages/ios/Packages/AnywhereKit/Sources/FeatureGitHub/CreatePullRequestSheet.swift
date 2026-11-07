import DesignSystem
import Stores
import SwiftUI
import Wire

/// Pushes the branch and opens its PR: what it opens into, its title, description, draft and base.
@MainActor
struct CreatePullRequestSheet: View {
    let store: GitHubStore
    let model: TreePullRequest
    let newPr: NewPr
    let branches: (@MainActor () async throws -> BranchList)?

    @State private var title: String
    @State private var bodyText: String
    /// Nil opens it into the repository's default branch.
    @State private var base: String?
    @State private var error: String?
    @AppStorage("github.draft") private var draft = false
    @Environment(\.dismiss) private var dismiss

    init(store: GitHubStore, tree: String, newPr: NewPr, branches: (@MainActor () async throws -> BranchList)? = nil) {
        self.store = store
        model = store.pullRequest(tree)
        self.newPr = newPr
        self.branches = branches
        let filled = PullRequestText.fromCommits(newPr.commits)
        _title = State(initialValue: filled.title)
        _bodyText = State(initialValue: filled.body)
        _base = State(initialValue: newPr.base.map(PullRequestText.baseBranch))
    }

    var body: some View {
        NavigationStack {
            Form {
                if let branch = newPr.branch {
                    Section {
                        let onBase = base == newPr.base.map(PullRequestText.baseBranch)
                        Label(PullRequestText.headLine(branch: branch, base: base ?? "default branch", ahead: onBase ? newPr.ahead : nil),
                              systemImage: "arrow.triangle.pull")
                            .font(Typography.code)
                            .foregroundStyle(Palette.text2.color)
                            .lineLimit(1)
                            .truncationMode(.middle)
                    }
                }
                Section {
                    TextField("Title", text: $title)
                    TextField("What this changes, and why", text: $bodyText, axis: .vertical).lineLimit(6...16)
                }
                Section {
                    Toggle("Draft", isOn: $draft)
                    NavigationLink {
                        BasePicker(branches: branches, selection: $base)
                    } label: {
                        LabeledContent("Base", value: base ?? "The repository's default")
                    }
                } footer: {
                    Text("Pushes the branch first.")
                }
                if let error {
                    Section { Text(error).foregroundStyle(Palette.failedText.color) }
                }
            }
            .formStyle(.grouped)
            .navigationTitle("New pull request")
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                ToolbarItem(placement: .confirmationAction) {
                    Button(PullRequestText.createLabel(draft: draft, busy: model.busy)) { create() }
                        .disabled(title.trimmingCharacters(in: .whitespaces).isEmpty || model.busy != nil)
                }
            }
            .interactiveDismissDisabled(model.busy != nil)
        }
    }

    private func create() {
        error = nil
        Task {
            do {
                _ = try await store.create(model.tree, title: title.trimmingCharacters(in: .whitespaces), body: bodyText, draft: draft, base: base)
                Haptics.success()
                dismiss()
            } catch {
                Haptics.warning()
                self.error = GitHubStore.describe(error)
            }
        }
    }
}
