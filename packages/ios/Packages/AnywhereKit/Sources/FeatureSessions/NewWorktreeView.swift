import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A worktree made on its own, without a session in it yet.
@MainActor
struct NewWorktreeView: View {
    let host: HostModel
    @State private var draft: NewSessionDraft
    @State private var setup = true
    @State private var requestId: String?
    @Environment(\.dismiss) private var dismiss

    init(host: HostModel, project: String) {
        self.host = host
        var draft = NewSessionDraft(project: project, place: .newWorktree)
        draft.seed = Int.random(in: 0..<1_000_000)
        _draft = State(initialValue: draft)
    }

    private var workspace: WorkspaceStore { host.workspace }
    private var project: Project? { workspace.project(draft.project) }
    private var mainTree: String { NewSessionView.main(of: project) ?? draft.project }
    private var taken: Set<String> { Places.taken(project: project, branches: workspace.branches[mainTree]) }
    private var problem: String? { draft.problem(taken: taken, maxAccess: Access.full.rawValue) }

    var body: some View {
        Form {
            if let requestId {
                CreationProgress(creation: workspace.creations[requestId]) {
                    workspace.forget(creation: requestId)
                    self.requestId = nil
                }
            } else {
                Section {
                    WorktreeFields(draft: $draft, branches: workspace.branches[mainTree], canChooseSource: true, taken: taken)
                } footer: {
                    if let problem { Text(problem).foregroundStyle(Palette.failedText.color) }
                }
                Section {
                    Toggle("Run setup", isOn: $setup)
                } footer: {
                    Text("The project's setup runs in its own terminal, which stays open.")
                }
            }
        }
        .formStyle(.grouped)
        .navigationTitle(project.map { "New Worktree in \($0.name)" } ?? "New Worktree")
        #if os(iOS)
            .navigationBarTitleDisplayMode(.inline)
        #endif
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Cancel") {
                    if let requestId { workspace.forget(creation: requestId) }
                    dismiss()
                }
            }
            ToolbarItem(placement: .confirmationAction) {
                Button("Create", action: create).disabled(requestId != nil || problem != nil)
            }
        }
        .task { _ = try? await workspace.loadBranches(tree: mainTree) }
    }

    private func create() {
        let id = WorkspaceStore.newRequestId()
        var request = draft.worktreeCreate(taken: taken, setup: setup)
        request.requestId = id
        requestId = id
        Task {
            do {
                _ = try await workspace.createWorktree(request)
                Haptics.success()
                workspace.forget(creation: id)
                dismiss()
            } catch {
                Haptics.warning()
            }
        }
    }
}
