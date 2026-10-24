import Wire

extension Sample {
    /// A long timeline holding every item kind and every tool kind, with sub-agents, repeated in turns until it has `turns` of them.
    public static func timeline(agentId: String = "a-working", turns: Int = 40) -> AgentTimeline {
        var items: [TimelineItem] = []
        var seq = 0
        func add(_ body: TimelineBody, parent: String? = nil) {
            seq += 1
            items.append(TimelineItem(id: "i\(seq)", seq: seq, ts: now - (turns * 20 - seq) * 15_000, parentToolUseId: parent, body: body))
        }
        for turn in 0..<turns {
            let t = "\(turn)"
            add(.user(text: turn == 0 ? "Port the commit graph lanes from crates/git/src/graph.rs, with its tests." : prompts[turn % prompts.count]))
            add(.thinking(text: "The lanes algorithm keeps one slot per open parent. A merge opens a lane; a commit whose sha fills a slot closes it.\n\nI should read graph.rs before writing anything."))
            add(.tool(ToolCall(toolUseId: "read-\(t)", name: "Read", detail: .read(path: "packages/desktop/crates/git/src/graph.rs"), status: .ok, output: "312 lines", durationMs: 12)))
            add(.tool(ToolCall(toolUseId: "grep-\(t)", name: "Grep", detail: .search(query: "Lanes::push", path: "packages/desktop/crates"), status: .ok, durationMs: 40)))
            add(.tool(ToolCall(toolUseId: "ls-\(t)", name: "Bash", detail: .shell(command: "swift test --filter DiffModelTests", description: "Run the diff model tests"), status: turn % 5 == 3 ? .error : .ok,
                               output: turn % 5 == 3 ? "error: no such module 'Wire'\n\(failureTail)" : "Test run with 12 tests passed", durationMs: 8_400)))
            add(.tool(ToolCall(toolUseId: "edit-\(t)", name: "Edit", detail: .edit(path: "Sources/DiffModel/Lanes.swift", diff: editDiff), status: .ok, durationMs: 6)))
            add(.tool(ToolCall(toolUseId: "write-\(t)", name: "Write", detail: .write(path: "Tests/DiffModelTests/LanesTests.swift", diff: writeDiff), status: .ok, durationMs: 4)))
            if turn % 4 == 0 {
                add(.tasks([
                    TaskItem(text: "Read graph.rs and its tests", status: .completed), TaskItem(text: "Port Lanes::push", status: .inProgress),
                    TaskItem(text: "Port the tests as swift-testing", status: .pending),
                ]))
                add(.plan(text: "1. Port `Lanes` as a value type.\n2. Port each test as a sentence.\n3. Bench 10 000 commits in release."))
                let task = "task-\(t)"
                add(.tool(ToolCall(toolUseId: task, name: "Agent", detail: .task(description: "Find every caller of Lanes", model: "claude-haiku-4-5", tokens: 18_508), status: .ok,
                                   output: "Two callers: history.rs and log_view.rs", durationMs: 9_100)))
                add(.user(text: "Find every caller of Lanes in packages/desktop and report the files."), parent: task)
                add(.tool(ToolCall(toolUseId: "sub-grep-\(t)", name: "Grep", detail: .search(query: "Lanes", path: nil), status: .ok)), parent: task)
                add(.assistant(text: "Two callers: `history.rs` and `log_view.rs`."), parent: task)
                add(.tool(ToolCall(toolUseId: "mcp-\(t)", name: "mcp__linear__create_issue", detail: .other(name: "mcp__linear__create_issue", input: .object(["title": .string("Port lanes")]), description: nil), status: .ok)))
                add(.tool(ToolCall(toolUseId: "stop-\(t)", name: "TaskStop", detail: .other(name: "TaskStop", input: .object(["task_id": .string(task)]), description: "Find every caller of Lanes"), status: .ok)))
            }
            if turn % 10 == 7 { add(.compact(trigger: turn % 20 == 7 ? "auto" : "manual")) }
            if turn % 6 == 2 { add(.notice(text: "Stop hook: 3 tests fail", error: nil)) }
            if turn % 9 == 4 { add(.notice(text: nil, error: "API Error: 529 overloaded")) }
            add(.assistant(text: answers[turn % answers.count]))
            let failed = turn % 7 == 6
            add(.result(ok: !failed, durationMs: 42_000 + turn * 1_300, error: failed ? "interrupted" : nil,
                        usage: TurnUsage(inputTokens: 12_000 + turn * 400, outputTokens: 1_800, cacheReadTokens: 9_000)))
        }
        return AgentTimeline(agentId: agentId, epoch: 1, items: items, hasOlder: false, maxSeq: seq)
    }

    static let prompts = [
        "Now port the tests.", "Why does the third lane close early?", "Run the whole suite in release and show the timings.",
        "Rename `slot` to `lane` everywhere.", "Make the merge commit test cover an octopus merge.",
    ]

    static let answers = [
        """
        Ported `Lanes::push` to `DiffModel/Lanes.swift`. Each commit:

        1. takes the lane whose slot waits for its sha, else a new one;
        2. hands its first parent that lane;
        3. opens a lane for each other parent.

        | Test | Result |
        |---|---|
        | a_linear_history_keeps_one_lane | passed |
        | a_merge_opens_a_lane_and_its_parent_closes_it | passed |

        > [!NOTE]
        > Lanes are recycled from the left, as the desktop does.

        ```swift
        mutating func push(_ commit: LogCommit) -> Row {
            let lane = slots.firstIndex(of: commit.sha) ?? open()
            slots[lane] = commit.parents.first
            return Row(lane: lane, merges: commit.parents.dropFirst().map(open))
        }
        ```
        """,
        "The third lane closed early because the octopus merge's last parent was already in a slot. Fixed by checking every slot before opening one — see `Lanes.swift:41`.",
        """
        All green in release:

        - `DiffModelTests`: 12 tests, 4 ms
        - `ChatModelTests`: 31 tests, 9 ms
        - [x] lanes
        - [ ] word diff

        The 10 000-commit bench takes **3.1 ms** per frame of rows, under the 8 ms budget.[^1]

        [^1]: Measured with `swift build -c release` on an M3.
        """,
    ]

    static let failureTail = (1...40).map { "  at frame \($0) in Lanes.swift" }.joined(separator: "\n")

    static let editDiff = FileDiff(
        lines: [
            DiffLine(number: 40, kind: .context, text: "    mutating func push(_ commit: LogCommit) -> Row {"),
            DiffLine(number: 41, kind: .del, text: "        let lane = open()"),
            DiffLine(number: 41, kind: .add, text: "        let lane = slots.firstIndex(of: commit.sha) ?? open()"),
            DiffLine(number: 42, kind: .context, text: "        slots[lane] = commit.parents.first"),
            DiffLine(number: 90, kind: .del, text: "    var slot: [String?] = []"),
            DiffLine(number: 90, kind: .add, text: "    var slots: [String?] = []"),
        ],
        additions: 2, deletions: 2)

    static let writeDiff = FileDiff(
        lines: (1...30).map { DiffLine(number: $0, kind: .add, text: $0 == 1 ? "import Testing" : "    #expect(lanes.push(commit\($0)).lane == \($0 % 3))") },
        additions: 30, deletions: 0, contentOnly: true)
}
