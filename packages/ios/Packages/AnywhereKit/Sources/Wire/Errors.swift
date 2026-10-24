/// `error {id?, message, code?, detail?}`: a refused request, or a failure the client acts on by `code`.
public struct ServerError: Codable, Hashable, Sendable, Error {
    public var message: String
    public var code: ErrorCode?
    public var detail: String?
    /// The id of the message it refuses, when that message was sent without waiting for its reply. Set by the
    /// connection, never coded: payloads leave the id to `ServerFrame`.
    public var replyTo: String?

    public init(message: String, code: ErrorCode? = nil, detail: String? = nil, replyTo: String? = nil) {
        self.message = message
        self.code = code
        self.detail = detail
        self.replyTo = replyTo
    }

    private enum CodingKeys: String, CodingKey { case message, code, detail }
}

public struct ErrorCode: OpenString {
    public let rawValue: String
    public init(rawValue: String) { self.rawValue = rawValue }

    public static let scopeDenied: Self = "scope_denied"
    public static let askOpen: Self = "ask_open"
    public static let promptRefused: Self = "prompt_refused"
    public static let promptTooLarge: Self = "prompt_too_large"
    public static let notOwnTerminal: Self = "not_own_terminal"
    public static let notPaired: Self = "not_paired"
    public static let revoked: Self = "revoked"
    public static let rateLimited: Self = "rate_limited"
    public static let tailnetOff: Self = "tailnet_off"
    public static let clientTooOld: Self = "client_too_old"
    public static let serverTooOld: Self = "server_too_old"
    public static let pairExpired: Self = "pair_expired"
    public static let pairUsed: Self = "pair_used"
    public static let pairInvalid: Self = "pair_invalid"
    public static let pairLocked: Self = "pair_locked"
    public static let pairFailed: Self = "pair_failed"
    public static let agentResuming: Self = "agent_resuming"
    public static let unknownProject: Self = "unknown_project"
    public static let unknownWorktree: Self = "unknown_worktree"
    public static let invalidName: Self = "invalid_name"
    public static let worktreeExists: Self = "worktree_exists"
    public static let invalidSpec: Self = "invalid_spec"
    public static let providerUnavailable: Self = "provider_unavailable"
    public static let accessNotAllowed: Self = "access_not_allowed"
    public static let folderNotTrusted: Self = "folder_not_trusted"
    public static let spawnFailed: Self = "spawn_failed"
    public static let duplicate: Self = "duplicate"
    public static let invalidConfig: Self = "invalid_config"
    public static let unknownLocal: Self = "unknown_local"
    public static let unknownAccount: Self = "unknown_account"
    public static let notATree: Self = "not_a_tree"
    public static let outsideTree: Self = "outside_tree"
    public static let terminalsOff: Self = "terminals_off"
    public static let terminalEnded: Self = "terminal_ended"
    public static let mainWorktree: Self = "main_worktree"
    public static let gitFailed: Self = "git_failed"
    public static let ghFailed: Self = "gh_failed"
    public static let ghMissing: Self = "gh_missing"
    public static let ghSignedOut: Self = "gh_signed_out"
    public static let attachInvalid: Self = "attach_invalid"
    public static let invalidAutomation: Self = "invalid_automation"
    public static let unknownAutomation: Self = "unknown_automation"
    public static let automationBusy: Self = "automation_busy"
}

/// The text an old pocketd answers any message it can't read with.
public let malformedMessage = "Malformed message"
