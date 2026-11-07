import DesignSystem
import Domain
import Stores
import SwiftUI
import Wire

/// The branch Compare is against: the base first, then the tree's branches, local ones first, searchable.
@MainActor
struct ComparePicker: View {
    let host: HostModel
    let tree: String
    let base: String?
    /// The branch picked; nil is the base.
    @Binding var picked: String?
    @State private var branches: BranchList?
    @State private var error: String?
    @State private var search = ""
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        let listed = WorktreeName.listed(local: branches?.local.map(\.name) ?? [], remote: branches?.remote.map(\.name) ?? [], search: search)
        NavigationStack {
            List {
                if let base, search.isEmpty {
                    row(nil, label: base, detail: "The base")
                }
                ForEach(listed.filter { $0.name != base }, id: \.name) { branch in
                    row(branch.name, label: branch.name, detail: branch.local ? nil : "On origin")
                }
            }
            .overlay {
                if let error {
                    Placeholder("Couldn't list branches", systemImage: "exclamationmark.triangle", detail: error) { Task { await load() } }
                } else if branches == nil {
                    ProgressView()
                } else if listed.isEmpty && !search.isEmpty {
                    ContentUnavailableView.search(text: search)
                }
            }
            .searchable(text: $search, prompt: "Branches")
            .navigationTitle("Compare Against")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
        }
    }

    private func load() async {
        error = nil
        do {
            branches = try await host.workspace.loadBranches(tree: tree)
        } catch {
            self.error = ErrorText.describe(error, mac: host.name)
        }
    }

    private func row(_ value: String?, label: String, detail: String?) -> some View {
        BranchRow(label, detail: detail, picked: picked == value) {
            picked = value
            dismiss()
        }
    }
}
