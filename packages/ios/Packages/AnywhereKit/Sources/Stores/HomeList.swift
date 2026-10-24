import Domain
import Wire

/// The home list: each project with its own checkout, Locals and worktrees, each with its sessions, as the desktop
/// sidebar groups them (`pocket/src/sidebar.rs`).
public enum HomeList {
    /// What placing and sorting a session needs.
    public struct Session: Hashable, Sendable {
        public var card: Sidebar.Card
        public var cwd: String
        public var terminalId: String

        public init(card: Sidebar.Card, cwd: String, terminalId: String) {
            self.card = card
            self.cwd = cwd
            self.terminalId = terminalId
        }
    }

    public struct RollUp: Hashable, Sendable {
        public var status: Status
        public var count: Int
    }

    public struct Tree: Hashable, Sendable, Identifiable {
        public enum Kind: Hashable, Sendable { case own, local, worktree }
        /// A worktree's path, or a Local's id.
        public var id: String
        public var kind: Kind
        public var label: String
        /// A worktree's branch, or its folder when detached.
        public var place: String?
        /// Session ids in the tree's order.
        public var sessions: [String]
        public var rollUp: RollUp?
    }

    public struct Section: Hashable, Sendable, Identifiable {
        /// The project's path.
        public var id: String
        public var name: String
        public var trees: [Tree]
        public var rollUp: RollUp?
    }

    public struct Built: Hashable, Sendable {
        public var sections: [Section]
        /// Sessions in no project, newest first.
        public var elsewhere: [String]
    }

    /// `localOfTerminal` places a session opened in a Local there; the rest go to the tree holding their folder.
    public static func build(
        projects: [Project], names: [String: String], locals: [Local], localOfTerminal: [String: String], sessions: [Session],
        query: String = "", sort: Sidebar.SessionSort = .newest
    ) -> Built {
        let index = Index(projects)
        let localIds = Set(locals.map(\.id))
        var byTree: [String: [Sidebar.Card]] = [:]
        var elsewhere: [Sidebar.Card] = []
        for s in sessions {
            if let local = localOfTerminal[s.terminalId], localIds.contains(local) {
                byTree[local, default: []].append(s.card)
            } else if let tree = index.tree(of: s.cwd) {
                byTree[tree, default: []].append(s.card)
            } else {
                elsewhere.append(s.card)
            }
        }
        let localsOf = Dictionary(grouping: locals, by: \.project)
        let q = query.trimmingCharacters(in: .whitespaces).lowercased()
        var sections: [Section] = []
        for project in projects {
            let main = project.worktrees.first(where: \.isMain)?.path ?? project.path
            var trees = [Tree(id: main, kind: .own, label: Sidebar.ownLocalName(given: names[main]),
                              place: project.worktrees.first(where: \.isMain).map { Sidebar.treePlace(path: $0.path, branch: $0.branch) }, sessions: [])]
            trees += (localsOf[project.path] ?? []).map { Tree(id: $0.id, kind: .local, label: $0.name, sessions: []) }
            for w in project.worktrees where !w.isMain {
                let (label, place) = Sidebar.treeLabel(path: w.path, branch: w.branch, given: names[w.path])
                trees.append(Tree(id: w.path, kind: .worktree, label: label, place: place, sessions: []))
            }
            let all = trees.flatMap { byTree[$0.id] ?? [] }
            let projectMatches = q.isEmpty || project.name.lowercased().contains(q)
            var shown: [Tree] = []
            for var tree in trees {
                let cards = byTree[tree.id] ?? []
                let treeMatches = projectMatches || tree.label.lowercased().contains(q)
                let kept = treeMatches ? cards : Sidebar.matching(cards, q)
                guard treeMatches || !kept.isEmpty else { continue }
                tree.sessions = Sidebar.sorted(kept, by: sort).map(\.id)
                tree.rollUp = rollUp(cards)
                shown.append(tree)
            }
            guard !shown.isEmpty else { continue }
            sections.append(Section(id: project.path, name: project.name, trees: shown, rollUp: rollUp(all)))
        }
        let rest = Sidebar.matching(elsewhere, q).sorted { $0.createdAt != $1.createdAt ? $0.createdAt > $1.createdAt : $0.id < $1.id }
        return Built(sections: sections, elsewhere: rest.map(\.id))
    }

    static func rollUp(_ cards: [Sidebar.Card]) -> RollUp? {
        Sidebar.rollUp(cards.map(\.status)).map { RollUp(status: $0.status, count: $0.count) }
    }

    /// `Sidebar.treeOf` by walking up a folder's parents instead of testing every tree.
    struct Index {
        var worktrees: [String: String] = [:]
        var folders: [String: String] = [:]

        init(_ projects: [Project]) {
            for p in projects {
                let main = p.worktrees.first(where: \.isMain)?.path ?? p.path
                folders[p.path] = main
                for w in p.worktrees {
                    worktrees[w.path] = w.path
                    folders[w.path] = main
                }
            }
        }

        func tree(of cwd: String) -> String? {
            Self.walk(cwd, worktrees) ?? Self.walk(cwd, folders)
        }

        static func walk(_ cwd: String, _ map: [String: String]) -> String? {
            var path = Substring(cwd)
            while !path.isEmpty {
                if let hit = map[String(path)] { return hit }
                guard let slash = path.lastIndex(of: "/") else { return nil }
                path = path[..<slash]
            }
            return nil
        }
    }
}
