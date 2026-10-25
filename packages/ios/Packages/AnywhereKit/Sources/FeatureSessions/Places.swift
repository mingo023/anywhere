import Domain
import Stores
import Wire

/// Where a new session starts, as the desktop's page offers it: a new Worktree, or a new Local; and the workspace the
/// page was opened from, when it was.
enum Places {
    struct Option: Hashable, Identifiable {
        var place: NewSessionDraft.Place
        var label: String
        var hint: String?
        var systemImage: String
        var id: NewSessionDraft.Place { place }
    }

    /// A new Local takes workspace.v1; a new worktree takes spawn only.
    static func options(project: Project, opened: NewSessionDraft.Place?, locals: [Local], names: [String: String], canManage: Bool, canSpawn: Bool) -> [Option] {
        var options: [Option] = []
        switch opened {
        case .tree(let path):
            let tree = project.worktrees.first { $0.path == path }
            if tree?.isMain ?? (path == project.path) {
                options.append(Option(place: .tree(path), label: Sidebar.ownLocalName(given: names[path]), systemImage: "laptopcomputer"))
            } else if let tree {
                let label = Sidebar.treeLabel(path: path, branch: tree.branch, given: names[path]).label
                options.append(Option(place: .tree(path), label: label, systemImage: "arrow.triangle.branch"))
            }
        case .local(let id):
            if let local = locals.first(where: { $0.id == id && $0.project == project.path }) {
                options.append(Option(place: .local(id), label: local.name, systemImage: "folder"))
            }
        default: break
        }
        if canSpawn { options.append(Option(place: .newWorktree, label: "Worktree", hint: "Its own branch and copy of the files", systemImage: "arrow.triangle.branch")) }
        if canManage {
            options.append(Option(place: .newLocal, label: "Local", hint: "A new place over the project's checkout: files and branch are shared", systemImage: "laptopcomputer"))
        }
        return options
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
