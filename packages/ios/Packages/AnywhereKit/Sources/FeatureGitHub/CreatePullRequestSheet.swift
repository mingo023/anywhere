import DesignSystem
import Stores
import SwiftUI
import Wire

/// Pushes the branch and opens its PR: title, description, draft and base.
@MainActor
struct CreatePullRequestSheet: View {
    let store: GitHubStore
    let model: TreePullRequest

    @State private var title: String
    @State private var bodyText: String
    @State private var draft = false
    @State private var base = ""
    @State private var error: String?
    @Environment(\.dismiss) private var dismiss

    init(store: GitHubStore, tree: String, commits: [CommitRef], base: String? = nil) {
        self.store = store
        model = store.pullRequest(tree)
        let filled = PullRequestText.fromCommits(commits)
        _title = State(initialValue: filled.title)
        _bodyText = State(initialValue: filled.body)
        _base = State(initialValue: base ?? "")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("Title", text: $title)
                    TextField("What this changes, and why", text: $bodyText, axis: .vertical).lineLimit(6...16)
                }
                Section {
                    Toggle("Draft", isOn: $draft)
                    TextField("Base branch", text: $base, prompt: Text("The repository's default"))
                        .autocorrectionDisabled()
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
        let base = base.trimmingCharacters(in: .whitespaces)
        Task {
            do {
                _ = try await store.create(model.tree, title: title.trimmingCharacters(in: .whitespaces), body: bodyText, draft: draft,
                                           base: base.isEmpty ? nil : base)
                Haptics.success()
                dismiss()
            } catch {
                Haptics.warning()
                self.error = GitHubStore.describe(error)
            }
        }
    }
}
