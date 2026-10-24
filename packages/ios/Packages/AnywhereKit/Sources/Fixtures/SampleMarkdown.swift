/// Markdown and source code for the markdown and highlight tests, gallery and bench.
extension Sample {
    /// Every element the chat's markdown draws.
    public static let kitchenSink = #"""
        # Shipping the phone terminal

        The daemon now streams **binary frames** for terminal output, so `pocketd` never base64-encodes bytes on the hot path.
        See `packages/pocketd/internal/wsserver/terminal.go` and the [protocol notes](docs/mobile/protocol.md#terminal), or
        read https://github.com/migueldeicaza/SwiftTerm for the view. Mixed marks: *italic*, ***both***, ~~struck~~ and <u>underlined</u>.
        A line ending in two spaces
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

    /// Markdown at least `bytes` long, mixing every block kind, with no reference definitions so it streams cheaply.
    public static func markdownDocument(bytes: Int) -> String {
        var out = ""
        var n = 0
        while out.utf8.count < bytes {
            n += 1
            out += """
                ## Section \(n): the \(["diff", "terminal", "chat", "git", "push"][n % 5]) pane

                Paragraph \(n) explains *why* the change matters, with `src/section\(n).rs`, a [link](https://example.com/\(n)) and **strong text** that wraps
                over several source lines so the parser has soft breaks to fold, and some inline `code()` too.

                - item one of \(n)
                - item two with `inline` code
                  - nested \(n)
                - [x] a finished task

                | key | value | note |
                |:--|--:|---|
                | a\(n) | \(n * 7) | **bold** |
                | b\(n) | \(n * 11) | `x \\| y` |

                > [!NOTE]
                > Alert \(n) holds a list:
                > 1. first
                > 2. second

                ```rust
                fn section_\(n)(input: &str) -> Result<usize, Error> {
                    let parsed = input.parse::<usize>()?; // step \(n)
                    Ok(parsed * \(n))
                }
                ```


                """
        }
        return out
    }

    static let typescriptChunk = #"""
        import { DaemonConnection, type TimelineItem } from "./connection";
        import type { AgentSummary, Status } from "@anywhere/protocol";

        /** Folds pocketd items into turns. See `chat/src/timeline.rs`. */
        export interface Turn<T extends TimelineItem = TimelineItem> {
          readonly id: string;
          prompt?: T;
          blocks: Array<T>;
          endedAt: number | null;
        }

        const STATUS_ORDER: Record<Status, number> = { needsYou: 0, failed: 1, working: 2, done: 3, idle: 4 };
        const PATH = /^(?:[a-z]+:\/\/)?[\w.-]+(?:\/[\w.-]*)*$/i;

        export class Timeline {
          private turns = new Map<string, Turn>();
          #revision = 0;

          constructor(private readonly connection: DaemonConnection, public agent: AgentSummary) {}

          async load(limit = 500): Promise<Turn[]> {
            const items = await this.connection.request<TimelineItem[]>("agent.timeline", { agentId: this.agent.id, limit });
            for (const item of items.sort((a, b) => a.seq - b.seq)) {
              const turn = this.turns.get(item.turnId) ?? { id: item.turnId, blocks: [], endedAt: null };
              if (item.kind === "user") turn.prompt = item;
              else turn.blocks.push(item);
              this.turns.set(turn.id, turn);
            }
            this.#revision += 1;
            return [...this.turns.values()];
          }

          describe(turn: Turn): string {
            const count = turn.blocks.length;
            return `Turn ${turn.id}: ${count} block${count === 1 ? "" : "s"}, ended ${turn.endedAt ? new Date(turn.endedAt).toISOString() : "never"}`;
          }

          static rank(a: AgentSummary, b: AgentSummary): number {
            return STATUS_ORDER[a.status] - STATUS_ORDER[b.status] || b.updatedAt - a.updatedAt || a.title.localeCompare(b.title);
          }
        }

        export function isPath(text: string): boolean {
          return !/\s/.test(text) && PATH.test(text) && text.length < 0x400 && text !== "." && text.split("/").length > 1;
        }


        """#

    static let rustChunk = #"""
        use std::collections::HashMap;
        use std::ops::Range;

        /// Byte ranges of top-level markdown blocks, never splitting a fenced code block.
        #[derive(Debug, Clone, PartialEq, Eq, Default)]
        pub struct Blocks<'a> {
            text: &'a str,
            ranges: Vec<Range<usize>>,
            cache: HashMap<String, (usize, bool)>,
        }

        impl<'a> Blocks<'a> {
            pub fn new(text: &'a str) -> Self {
                Self { text, ranges: markdown_blocks(text), cache: HashMap::new() }
            }

            /// The block holding byte `offset`, if any.
            pub fn at(&self, offset: usize) -> Option<&str> {
                self.ranges.iter().find(|r| r.contains(&offset)).map(|r| &self.text[r.clone()])
            }
        }

        pub fn markdown_blocks(text: &str) -> Vec<Range<usize>> {
            let mut blocks = Vec::new();
            let mut current: Option<Range<usize>> = None;
            let mut fence: Option<(u8, usize)> = None;
            let (mut list, mut after_blank) = (false, false);
            let mut start = 0;
            for line in text.split_inclusive('\n') {
                let range = start..start + line.trim_end_matches(['\n', '\r']).len();
                start += line.len();
                let content = &text[range.clone()];
                if let Some((ch, len)) = fence {
                    current.as_mut().expect("a fence is open").end = range.end;
                    if closes_fence(content, ch, len) {
                        fence = None;
                        blocks.extend(current.take());
                    }
                    continue;
                }
                /* Blank lines end a block, /* unless */ a list carries on. */
                if content.trim().is_empty() {
                    after_blank = true;
                    continue;
                }
                let raw = r#"a "raw" string"#;
                let bytes = b"\x00\xff";
                debug_assert!(raw.len() > 0 && bytes.len() == 2, "{} {:?}", 0xFF_u8, 1.5e-3_f64);
                after_blank = false;
            }
            blocks.extend(current);
            blocks
        }

        fn closes_fence(line: &str, ch: u8, len: usize) -> bool {
            let run = line.bytes().take_while(|&c| c == ch).count();
            run >= len && line[run..].trim().is_empty()
        }


        """#

    /// TypeScript source at least `bytes` long.
    public static func typescriptSource(bytes: Int) -> String { repeated(typescriptChunk, bytes) }

    /// Rust source at least `bytes` long.
    public static func rustSource(bytes: Int) -> String { repeated(rustChunk, bytes) }

    static func repeated(_ chunk: String, _ bytes: Int) -> String {
        String(repeating: chunk, count: max(1, (bytes + chunk.utf8.count - 1) / chunk.utf8.count))
    }
}
