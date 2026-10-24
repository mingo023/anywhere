import Foundation
import Wire

/// Terminal screens as pocketd's snapshots carry them: VT bytes that redraw the screen.
extension Sample {
    /// A shell in the calm-otter worktree after a few commands.
    public static let shellScreen = Data((
        "\u{1b}[1;32mmingo@mbp\u{1b}[0m \u{1b}[1;34m~/dev/anywhere\u{1b}[0m \u{1b}[33m(main)\u{1b}[0m $ git status --short\r\n"
            + " \u{1b}[31mM\u{1b}[0m packages/pocketd/internal/wsserver/terminal.go\r\n"
            + " \u{1b}[31mM\u{1b}[0m packages/ios/Packages/AnywhereKit/Sources/Stores/TerminalStore.swift\r\n"
            + "\u{1b}[32m??\u{1b}[0m packages/pocketd/e2e/terminal_test.go\r\n"
            + "\u{1b}[1;32mmingo@mbp\u{1b}[0m \u{1b}[1;34m~/dev/anywhere\u{1b}[0m \u{1b}[33m(main)\u{1b}[0m $ go test ./internal/wsserver/\r\n"
            + "ok  \tpocketd/internal/wsserver\t3.227s\r\n"
            + "\u{1b}[1;32mmingo@mbp\u{1b}[0m \u{1b}[1;34m~/dev/anywhere\u{1b}[0m \u{1b}[33m(main)\u{1b}[0m $ ls\r\n"
            + "\u{1b}[1;34mdocs\u{1b}[0m      \u{1b}[1;34mpackages\u{1b}[0m  \u{1b}[1;34mscripts\u{1b}[0m   CLAUDE.md  CONTEXT.md  Makefile  README.md\r\n"
            + "\u{1b}[1;32mmingo@mbp\u{1b}[0m \u{1b}[1;34m~/dev/anywhere\u{1b}[0m \u{1b}[33m(main)\u{1b}[0m $ "
    ).utf8)

    /// Claude Code mid-turn in a terminal.
    public static let claudeScreen = Data((
        "\u{1b}[38;5;173m╭───────────────────────────────────────────────╮\u{1b}[0m\r\n"
            + "\u{1b}[38;5;173m│\u{1b}[0m \u{1b}[1m✻ Welcome to Claude Code!\u{1b}[0m                     \u{1b}[38;5;173m│\u{1b}[0m\r\n"
            + "\u{1b}[38;5;173m│\u{1b}[0m   cwd: ~/.worktrees/anywhere/calm-otter       \u{1b}[38;5;173m│\u{1b}[0m\r\n"
            + "\u{1b}[38;5;173m╰───────────────────────────────────────────────╯\u{1b}[0m\r\n\r\n"
            + "\u{1b}[2m> \u{1b}[0mPort the commit graph lanes\r\n\r\n"
            + "\u{1b}[38;5;173m⏺\u{1b}[0m Reading \u{1b}[1mcrates/git/src/graph.rs\u{1b}[0m\r\n"
            + "\u{1b}[38;5;173m⏺\u{1b}[0m \u{1b}[1mUpdate\u{1b}[0m(Sources/DiffModel/Lanes.swift)\r\n"
            + "  ⎿  Updated with \u{1b}[32m42 additions\u{1b}[0m and \u{1b}[31m3 removals\u{1b}[0m\r\n\r\n"
            + "\u{1b}[38;5;209m✶\u{1b}[0m Porting… \u{1b}[2m(34s · ↑ 2.1k tokens · esc to interrupt)\u{1b}[0m\r\n"
    ).utf8)

    /// `count` bytes of a test run's colored log, as a busy terminal streams it.
    public static func terminalLog(bytes count: Int) -> Data {
        var out = Data(capacity: count + 128)
        var n = 0
        while out.count < count {
            let line = n % 7 == 0
                ? "\u{1b}[31mFAIL\u{1b}[0m src/timeline/\(n % 97).test.ts \u{1b}[2m(\(n % 400) ms)\u{1b}[0m\r\n"
                : "\u{1b}[32mPASS\u{1b}[0m src/components/Row\(n % 211).test.tsx \u{1b}[2m(\(n % 90) ms)\u{1b}[0m ✓ renders\r\n"
            out.append(contentsOf: line.utf8)
            n += 1
        }
        return out.prefix(count)
    }
}
