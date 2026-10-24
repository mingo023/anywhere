import DesignSystem
import Domain
import Stores
import SwiftUI

/// Fuzzy go-to-file over the tree's tracked and untracked files; changed files lead an empty query.
@MainActor
struct GoToFileSheet: View {
    @State var search: FileSearch
    let pick: (String) -> Void
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            results
                .navigationTitle("Go to File")
                #if os(iOS)
                    .navigationBarTitleDisplayMode(.inline)
                    .searchable(text: $search.query, placement: .navigationBarDrawer(displayMode: .always), prompt: "File name")
                    .textInputAutocapitalization(.never)
                #else
                    .searchable(text: $search.query, prompt: "File name")
                #endif
                .autocorrectionDisabled()
                .onSubmit(of: .search) { if let first = search.results.first { pick(first) } }
                .toolbar {
                    ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
                }
        }
        .presentationDetents([.large])
        .presentationDragIndicator(.visible)
        .task { search.start() }
    }

    @ViewBuilder
    private var results: some View {
        if search.results.isEmpty, !search.searching {
            if let failure = search.failure {
                ContentUnavailableView("Can't search", systemImage: "exclamationmark.triangle", description: Text(failure))
            } else if search.query.isEmpty {
                ContentUnavailableView("No files", systemImage: "doc")
            } else {
                ContentUnavailableView.search(text: search.query)
            }
        } else {
            List(search.results, id: \.self) { path in
                Button { pick(path) } label: { SearchHitRow(hit: SearchHit(path: path, query: search.query)).equatable() }
                    .buttonStyle(.plain)
                    .listRowBackground(Palette.background.color)
            }
            .listStyle(.plain)
            .scrollContentBackground(.hidden)
            .scrollDismissesKeyboard(.immediately)
            .screenBackground()
            .overlay(alignment: .top) {
                if search.searching { ProgressView().controlSize(.small).padding(.top, Space.s) }
            }
        }
    }
}

@MainActor
struct SearchHitRow: View, Equatable {
    let hit: SearchHit

    var body: some View {
        HStack(spacing: Space.m) {
            Image(systemName: "doc.text").foregroundStyle(Palette.text3.color).frame(width: 20)
            VStack(alignment: .leading, spacing: 2) {
                Text(Self.marked(hit)).font(Typography.body).foregroundStyle(Palette.text.color).lineLimit(1)
                if !hit.folder.isEmpty {
                    Text(hit.folder).font(Typography.footnote).foregroundStyle(Palette.text3.color).lineLimit(1).truncationMode(.head)
                }
            }
        }
        .padding(.vertical, Space.xs)
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(Rectangle())
    }

    /// The name with its matched letters in the accent colour and bold.
    static func marked(_ hit: SearchHit) -> AttributedString {
        var out = AttributedString()
        let marked = Set(hit.marked)
        for (i, c) in hit.name.enumerated() {
            var piece = AttributedString(String(c))
            if marked.contains(i) {
                piece.foregroundColor = Palette.accent.color
                piece.inlinePresentationIntent = .stronglyEmphasized
            }
            out += piece
        }
        return out
    }
}
