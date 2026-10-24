import DesignSystem
import Stores
import SwiftUI
import Wire

/// Picks where a new shell opens: a project's worktree, or one of its Locals.
@MainActor
struct NewTerminalSheet: View {
    let store: TerminalStore
    let created: (String) -> Void
    @State private var projects: [Project]?
    @State private var opening: String?
    @State private var problem: String?
    @Environment(\.dismiss) private var dismiss

    init(store: TerminalStore, projects: [Project]? = nil, created: @escaping (String) -> Void) {
        self.store = store
        self.created = created
        _projects = State(initialValue: projects)
    }

    var body: some View {
        NavigationStack {
            Group {
                if let projects {
                    List {
                        if let problem {
                            Text(problem).font(Typography.footnote).foregroundStyle(Palette.failedText.color)
                        }
                        ForEach(projects) { project in
                            Section(project.name) {
                                ForEach(project.worktrees, id: \.path) { tree in
                                    place(tree.name, detail: tree.branch.isEmpty ? nil : tree.branch,
                                          systemImage: tree.isMain ? "folder" : "arrow.triangle.branch", key: tree.path) {
                                        try await store.create(cwd: tree.path, localId: nil)
                                    }
                                }
                                ForEach(store.locals.filter { $0.project == project.path }) { local in
                                    place(local.name, detail: "Local", systemImage: "house", key: "local:" + local.id) {
                                        try await store.create(cwd: nil, localId: local.id)
                                    }
                                }
                            }
                        }
                    }
                    #if os(iOS)
                        .listStyle(.insetGrouped)
                    #endif
                } else {
                    ProgressView().frame(maxWidth: .infinity, maxHeight: .infinity)
                }
            }
            .navigationTitle("New Terminal")
            #if os(iOS)
                .navigationBarTitleDisplayMode(.inline)
            #endif
            .toolbar {
                ToolbarItem(placement: .cancellationAction) { Button("Cancel") { dismiss() } }
            }
            .task { await load() }
        }
        .presentationDetents([.medium, .large])
    }

    private func place(_ name: String, detail: String?, systemImage: String, key: String, open: @escaping () async throws -> String) -> some View {
        Button {
            Task { await start(key, open) }
        } label: {
            HStack(spacing: Space.m) {
                Image(systemName: systemImage).foregroundStyle(Palette.text2.color).frame(width: 24)
                VStack(alignment: .leading, spacing: 0) {
                    Text(name).foregroundStyle(Palette.text.color)
                    if let detail { Text(detail).font(Typography.caption).foregroundStyle(Palette.text3.color) }
                }
                Spacer()
                if opening == key { ProgressView().controlSize(.small) }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .disabled(opening != nil)
    }

    private func load() async {
        guard projects == nil else { return }
        do {
            projects = try await store.projects()
        } catch {
            projects = []
            problem = "Couldn't load your projects"
        }
    }

    private func start(_ key: String, _ open: () async throws -> String) async {
        opening = key
        defer { opening = nil }
        do {
            let id = try await open()
            Haptics.success()
            dismiss()
            created(id)
        } catch {
            Haptics.warning()
            problem = "Couldn't open a terminal there"
        }
    }
}
