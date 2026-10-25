import DesignSystem
import SwiftUI

/// Markdown screens for the gallery: every element in light and dark, a wide table, a long code block and the
/// half-written states a streaming reply passes through.
public enum MarkdownGallery {
    /// Every element the chat's markdown draws.
    public static let kitchenSink = #"""
        # Shipping the phone terminal

        The daemon now streams **binary frames** for terminal output, so `pocketd` never base64-encodes bytes on the hot path.
        See `packages/pocketd/internal/wsserver/terminal.go` and the [protocol notes](docs/mobile/protocol.md#terminal), or
        read https://github.com/migueldeicaza/SwiftTerm for the view. Mixed marks: *italic*, ***both***, ~~struck~~ and <u>underlined</u>.
        A line ending in a backslash\
        keeps its break; this one
        reflows. Footnotes count in order[^speed] and again[^speed], then[^design].

        ## What changed

        1. A `terminal.attach` verb with a size
        2. Output frames of up to 64 KiB
           - coalesced every 16 ms
           - [x] dropped when the phone falls behind
           - [ ] resumable after a reconnect
        3. Input with key chords

        - [x] Ship the verb
        - [ ] Write the docs

        > Terminal access stays off until the Mac turns it on.

        > [!NOTE]
        > Phones that lack the `terminal` cap keep their read-only view.

        > [!TIP] Faster pairing
        > Scan the code from **Settings → Phone**.

        > [!IMPORTANT]
        > Pairing tokens rotate weekly.

        > [!WARNING]
        > A wide terminal reflows on the phone.

        > [!CAUTION]
        > Revoking a phone closes its sessions.

        ### Timings

        | Path | Before | After | Change |
        |:-----|-------:|------:|:------:|
        | `git.status`, 1200 files | 67 ms | 3 ms | **22×** |
        | Click → diff shown | 490 ms | ~100 ms | 5× |
        | Pipe in a cell | `a \| b` | either \| or | — |

        ```swift
        // Coalesce output so one frame carries 16 ms of bytes.
        func flush(_ chunk: [UInt8]) async throws -> Int {
            let frame = Frame(kind: .output, payload: chunk)
            try await connection.send(frame.encoded())
            return chunk.count // https://swift.org
        }
        ```

        ```sh
        POCKET_HOME=$(mktemp -d) pocketd --listen 127.0.0.1:7777 # scratch home
        ```

            indented code is labelled text

        $$
        \sum_{i=1}^{n} i = \frac{n(n+1)}{2}
        $$

        Inline math stays literal: $e^{i\pi} + 1 = 0$, and so do prices like $5.

        <details>
        <summary>Raw HTML</summary>

        Rendered as markdown inside.

        </details>

        ![Diagram](https://example.com/diagram.png "Architecture")

        ---

        中文段落 **加粗** 和 `代码`. Emoji 👍🏽 🎉. العربية **عريض** עברית.

        [^speed]: Measured on an iPhone 15 Pro in release.
        [^design]: See `docs/adr/0007-native-ios-app.md`.
        """#

    /// A table too wide for a phone.
    public static let wideTable = """
        | Command | What it does | Default | Flags | Since | Notes |
        |---|---|:-:|---|--:|---|
        | `pocketd serve` | Starts the daemon and listens for desktops and phones on the tailnet | on | `--listen`, `--home`, `--no-pty` | 0.1.0 | Restart after changing the listen address |
        | `pocketd pair` | Prints a pairing link and QR code for a new phone | — | `--name`, `--expires 10m` | 0.1.9 | Links expire after ten minutes |
        | `pocketd agents` | Lists every agent with its status, model and worktree | — | `--json`, `--all` | 0.1.4 | Use `--all` to include finished ones |
        """

    /// A code block with lines far wider than a phone.
    public static let longCode = """
        ```typescript
        export async function loadTimeline(connection: DaemonConnection, agentId: string, options: { limit?: number; before?: number } = {}): Promise<TimelineItem[]> {
          const { limit = 500, before } = options; // pocketd pages at most 500 items per request, newest first
          const items = await connection.request<TimelineItem[]>("agent.timeline", { agentId, limit, before }, { timeoutMs: 15_000, retries: 2 });
          return items.filter((item) => item.kind !== "noop").sort((a, b) => a.seq - b.seq).map((item) => ({ ...item, text: item.text?.trimEnd() ?? "" }));
        }
        ```
        """

    /// A reply cut off mid-stream: inside an unclosed fence, then halfway through a table.
    public static let streamingFence = "Here's the fix:\n\n```swift\nfunc retry(_ attempt: Int) async {\n    let delay = min(8, 0.25 * pow(2, Double(attempt)))\n    try? await Task.sleep(for: .seconds(delay"
    public static let streamingTable = "| Step | Time |\n|---|--:|\n| decode | 2.6 ms |\n| fold | 1."

    @MainActor public static var scenes: [GalleryScene] {
        [
            scene("markdown-kitchen-sink", kitchenSink, style: .document, height: 3000, dark: false),
            scene("markdown-kitchen-sink-dark", kitchenSink, style: .document, height: 3000, dark: true),
            scene("markdown-chat", kitchenSink, style: .chat, height: 3000, dark: false),
            scene("markdown-compact-dark", kitchenSink, style: .compact, height: 2600, dark: true),
            scene("markdown-wide-table", wideTable, style: .chat, height: 420, dark: false),
            scene("markdown-wide-table-dark", wideTable, style: .chat, height: 420, dark: true),
            scene("markdown-long-code", longCode, style: .chat, height: 260, dark: false),
            scene("markdown-long-code-dark", longCode, style: .chat, height: 260, dark: true),
            scene("markdown-streaming-fence", streamingFence, style: .chat, height: 260, dark: false),
            scene("markdown-streaming-table-dark", streamingTable, style: .chat, height: 260, dark: true),
        ]
    }

    @MainActor static func scene(_ name: String, _ markdown: String, style: MarkdownStyle, height: CGFloat, dark: Bool) -> GalleryScene {
        GalleryScene(name, size: CGSize(width: 393, height: height), dark: dark) {
            AnyView(
                ScrollView {
                    MarkdownView(markdown, style: style).padding(16)
                }
                .background(Palette.background.color)
                .environment(\.colorScheme, dark ? .dark : .light))
        }
    }
}
