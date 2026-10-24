import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// A new worktree's name and where its branch comes from: a new one off a base, an existing branch, or a pull request.
/// `canChooseSource` is workspace.v1, without which only a new branch off the default base can be made.
@MainActor
struct WorktreeFields: View {
    @Binding var draft: NewSessionDraft
    let branches: BranchList?
    let canChooseSource: Bool
    let taken: Set<String>

    var body: some View {
        if canChooseSource {
            Picker("From", selection: kind) {
                Text("New Branch").tag(0)
                Text("Branch").tag(1)
                Text("Pull Request").tag(2)
            }
            .pickerStyle(.segmented)
        }
        switch draft.source {
        case .new:
            TextField("Name", text: $draft.name, prompt: Text(namePlaceholder)).plainEntry()
            if canChooseSource {
                NavigationLink {
                    BranchPicker(title: "Base", branches: branches, allowDefault: true, selection: $draft.base)
                } label: {
                    LabeledContent("Base", value: draft.base ?? "Default")
                }
            }
        case .branch(let branch):
            NavigationLink {
                BranchPicker(title: "Branch", branches: branches, allowDefault: false, selection: Binding(
                    get: { branch.isEmpty ? nil : branch }, set: { draft.source = .branch($0 ?? "") }))
            } label: {
                LabeledContent("Branch", value: branch.isEmpty ? "Choose" : branch)
            }
        case .pr(let typed):
            TextField("Pull request", text: Binding(get: { typed }, set: { draft.source = .pr($0) }), prompt: Text("Number or link")).plainEntry()
        }
    }

    private var namePlaceholder: String {
        draft.prompt.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || draft.base != nil
            ? draft.worktreeName(taken: taken) : "Named from your prompt"
    }

    private var kind: Binding<Int> {
        Binding(
            get: {
                switch draft.source {
                case .new: 0
                case .branch: 1
                case .pr: 2
                }
            },
            set: { kind in
                draft.source = [NewSessionDraft.Source.new, .branch(""), .pr("")][kind]
            })
    }
}

extension View {
    /// A field for names, branches and links: no capitals, no corrections.
    func plainEntry() -> some View {
        #if os(iOS)
            textInputAutocapitalization(.never).autocorrectionDisabled()
        #else
            autocorrectionDisabled()
        #endif
    }
}
