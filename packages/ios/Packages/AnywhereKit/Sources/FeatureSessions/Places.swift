import Domain
import Stores
import Wire

/// Where a new session starts, as the desktop's page offers it: a new Worktree, or a new Local.
enum Places {
    struct Option: Hashable, Identifiable {
        var place: NewSessionDraft.Place
        var label: String
        var hint: String?
        var systemImage: String
        var id: NewSessionDraft.Place { place }
    }

    /// A new Local takes workspace.v1; a new worktree takes spawn only.
    static func options(canManage: Bool, canSpawn: Bool) -> [Option] {
        var options: [Option] = []
        if canSpawn { options.append(Option(place: .newWorktree, label: "Worktree", hint: "Its own branch and copy of the files", systemImage: "arrow.triangle.branch")) }
        if canManage {
            options.append(Option(place: .newLocal, label: "Local", hint: "A new place over the project's checkout: files and branch are shared", systemImage: "laptopcomputer"))
        }
        return options
    }

    /// The workspace of `project` a session was asked for from its row, shown in place of the choice; nil for a new one
    /// or one of another project.
    static func existing(_ place: NewSessionDraft.Place, project: Project, locals: [Local], names: [String: String]) -> Option? {
        switch place {
        case .tree(let path):
            let tree = project.worktrees.first { $0.path == path }
            if tree?.isMain ?? (path == project.path) {
                return Option(place: place, label: Sidebar.ownLocalName(given: names[path]), systemImage: "laptopcomputer")
            }
            return tree.map { Option(place: place, label: Sidebar.treeLabel(path: path, branch: $0.branch, given: names[path]).label, systemImage: "arrow.triangle.branch") }
        case .local(let id):
            return locals.first { $0.id == id && $0.project == project.path }.map { Option(place: place, label: $0.name, systemImage: "folder") }
        default:
            return nil
        }
    }

    /// The branch a new worktree starts from when none is picked, as pocketd's `defaultBase` picks it; without the
    /// branch list (pocketd lacking workspace.v1), the branch the project's own checkout is on.
    static func defaultBase(project: Project?, branches: BranchList?) -> String? {
        let main = project?.worktrees.first(where: \.isMain)?.branch ?? ""
        guard let branches else { return main.isEmpty ? nil : main }
        return WorktreeName.defaultBase(branches.local.map(\.name), current: branches.current ?? main)
    }

    /// The names a new worktree can't take: its project's branches, local and on origin, and its worktrees' folders.
    static func taken(project: Project?, branches: BranchList?) -> Set<String> {
        WorktreeName.taken(
            local: branches?.local.map(\.name) ?? [], remote: branches?.remote.map(\.name) ?? [],
            folders: project?.worktrees.map { $0.path.split(separator: "/").last.map(String.init) ?? $0.name } ?? [])
    }
}
