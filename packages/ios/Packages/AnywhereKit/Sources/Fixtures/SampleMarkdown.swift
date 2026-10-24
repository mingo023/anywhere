/// Long markdown and source code for the markdown and highlight bench.
extension Sample {
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
