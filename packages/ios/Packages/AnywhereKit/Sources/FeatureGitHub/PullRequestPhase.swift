import Wire

/// What a tree's PR screen shows. Only gh saying the branch has none is `none`, so only that offers Create PR.
public enum PullRequestPhase: Hashable, Sendable {
    case card
    case ghMissing
    case ghSignedOut
    /// The repository has no GitHub remote.
    case noRemote
    /// gh failed and has never said anything for this tree.
    case unreachable
    case none
    /// The Mac couldn't be asked before anything loaded.
    case failed(String)
    case loading

    public static func of(hasPr: Bool, problem: GhState?, loaded: Bool, error: String?) -> Self {
        if hasPr { return .card }
        switch problem {
        case .missing?: return .ghMissing
        case .signedOut?: return .ghSignedOut
        case .noRemote?: return .noRemote
        case .some: return .unreachable
        case nil: break
        }
        if loaded { return .none }
        if let error { return .failed(error) }
        return .loading
    }
}
