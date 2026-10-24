import Domain
import Stores
import Wire

/// Where the new session sheet offers to start: the project's own checkout, its Locals and worktrees, then new ones.
enum Places {
    struct Option: Hashable, Identifiable {
        var place: NewSessionDraft.Place
        var label: String
        var systemImage: String
        var id: NewSessionDraft.Place { place }
    }

    /// A new Local and a worktree from a base, branch or PR take workspace.v1; a new worktree off the default base takes spawn only.
    static func options(project: Project, locals: [Local], names: [String: String], canManage: Bool, canSpawn: Bool) -> [Option] {
        let main = project.worktrees.first(where: \.isMain)?.path ?? project.path
        var options = [Option(place: .tree(main), label: Sidebar.ownLocalName(given: names[main]), systemImage: "laptopcomputer")]
        options += locals.filter { $0.project == project.path }.map { Option(place: .local($0.id), label: $0.name, systemImage: "folder") }
        options += project.worktrees.filter { !$0.isMain }.map {
            Option(place: .tree($0.path), label: Sidebar.treeLabel(path: $0.path, branch: $0.branch, given: names[$0.path]).label,
                   systemImage: "arrow.triangle.branch")
        }
        if canManage { options.append(Option(place: .newLocal, label: "New Local", systemImage: "folder.badge.plus")) }
        if canSpawn { options.append(Option(place: .newWorktree, label: "New worktree", systemImage: "plus.square.on.square")) }
        return options
    }

    /// The names a new worktree can't take: its project's branches, local and on origin, and its worktrees' folders.
    static func taken(project: Project?, branches: BranchList?) -> Set<String> {
        WorktreeName.taken(
            local: branches?.local.map(\.name) ?? [], remote: branches?.remote.map(\.name) ?? [],
            folders: project?.worktrees.map { $0.path.split(separator: "/").last.map(String.init) ?? $0.name } ?? [])
    }
}
