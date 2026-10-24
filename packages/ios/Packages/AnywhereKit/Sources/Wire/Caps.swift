/// Protocol versions and the optional features a hello negotiates, named `<area>.v<n>`.
public enum Caps {
    public static let protocolVersion = 3
    public static let protocolMin = 3
    public static let protocolMax = 3

    public static let pair = "pair.v1"
    public static let scopes = "scopes.v1"
    public static let host = "host.v1"
    public static let registry = "registry.v1"
    public static let summaryV2 = "summary.v2"
    public static let launch = "launch.v1"
    public static let restore = "restore.v1"
    public static let open = "open.v1"
    public static let names = "names.v1"
    public static let automations = "automations.v1"
    public static let locals = "locals.v1"
    public static let fork = "fork.v1"
    public static let accounts = "accounts.v1"
    public static let subagents = "subagents.v1"
    public static let settings = "settings.v1"
    public static let commands = "commands.v1"
    public static let access = "access.v1"
    public static let terminal = "terminal.v1"
    public static let workspace = "workspace.v1"
    public static let git = "git.v1"
    public static let gh = "gh.v1"
    public static let fs = "fs.v1"
    public static let push = "push.v1"

    /// Everything the phone speaks, sent in every hello.
    public static let phone: [String] = [
        pair, scopes, host, registry, summaryV2, launch, restore, open, names, automations, locals, fork, accounts,
        subagents, settings, commands, access, terminal, workspace, git, gh, fs, push,
    ]
}

/// Limits both sides hold to.
public enum Limits {
    public static let toolOutput = 64 * 1024
    public static let diffPreviewLines = 24
    public static let diffLineChars = 160
    public static let timelinePage = 500
    public static let gitStatusPage = 500
    public static let gitStatusPageMax = 2000
    public static let gitLogPage = 50
    public static let gitLogPageMax = 200
    public static let diffMaxLines = 4000
    public static let diffLineMaxChars = 1024
    public static let fsReadMax = 524_288
    public static let fsSearchLimit = 50
    public static let attachChunk = 512 * 1024
    public static let attachTotal = 20 * 1024 * 1024
}

/// The WebSocket close codes pocketd ends a socket with.
public enum CloseCode {
    public static let unpaired = 4401
    public static let versionMismatch = 4426
}
