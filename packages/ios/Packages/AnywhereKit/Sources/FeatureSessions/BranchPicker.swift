import DesignSystem
import Domain
import SwiftUI
import Wire

/// A searchable list of a project's branches, local ones first: the 20 newest, or 50 matches while searching.
@MainActor
struct BranchPicker: View {
    let title: String
    let branches: BranchList?
    /// Offers "Default", which leaves the pick to pocketd.
    let allowDefault: Bool
    @Binding var selection: String?
    @State private var search = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let listed = WorktreeName.listed(local: branches?.local.map(\.name) ?? [], remote: branches?.remote.map(\.name) ?? [], search: search)
        List {
            if allowDefault && search.isEmpty {
                row(nil, label: "Default", detail: "The project's usual base")
            }
            ForEach(listed, id: \.name) { branch in
                row(branch.name, label: branch.name, detail: branch.local ? nil : "On origin")
            }
        }
        .overlay {
            if branches == nil {
                ProgressView()
            } else if listed.isEmpty && !search.isEmpty {
                ContentUnavailableView.search(text: search)
            }
        }
        .searchable(text: $search, prompt: "Branches")
        .navigationTitle(title)
    }

    private func row(_ value: String?, label: String, detail: String?) -> some View {
        BranchRow(label, detail: detail, picked: selection == value) {
            selection = value
            dismiss()
        }
    }
}
