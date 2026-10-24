import Testing
import Wire

@testable import ChatModel

private func listed(_ names: [String]) -> [Command] { names.map { Command(name: $0) } }
private func names(_ rows: [Slash]) -> [String] { rows.map(\.name) }
private let claude = Adapter.for("claude")
private let codex = Adapter.for("codex")

@Test("a slash command is its name and argument")
func aSlashCommandIsItsNameAndArgument() {
    #expect(slash("  /model haiku \n")! == ("model", "haiku"))
    #expect(slash("/compact")! == ("compact", ""))
    #expect(slash("/goal tests pass\nand lint")! == ("goal", "tests pass\nand lint"))
}

@Test("a prompt that starts with a path is no command")
func aPromptThatStartsWithAPathIsNoCommand() {
    #expect(slash("/usage/report.md is empty") == nil)
    #expect(slash("/etc.conf") == nil)
    #expect(slash("/") == nil)
    #expect(slash("why /model?") == nil)
}

@Test("only clearing the goal skips the queue")
func onlyClearingTheGoalSkipsTheQueue() {
    #expect(skipsQueue("/goal clear"))
    #expect(skipsQueue("  /goal  clear "))
    #expect(!skipsQueue("/goal clear the tests"))
    #expect(!skipsQueue("/goal pass the tests"))
    #expect(!skipsQueue("goal clear"))
}

@Test("claude runs pickers and compact itself and leaves panels out")
func claudeRunsPickersAndCompactItselfAndLeavesPanelsOut() {
    #expect(claude.command("model", "") == .local(.model))
    #expect(claude.command("model", "haiku") == .local(.model))
    #expect(claude.command("compact", "") == .local(.compact))
    #expect(claude.command("btw", "why?") == .local(.btw))
    #expect(claude.command("mcp", "") == .local(.mcp))
    #expect(claude.command("compact", "keep the plan") == .send, "an instruction only Claude can take")
    #expect(claude.command("clear", "") == .unsupported)
    #expect(claude.command("cost", "") == .unsupported)
    #expect(claude.command("resume", "") == .unsupported)
    #expect(claude.command("add-dir", "") == .unsupported)
    #expect(claude.command("add-dir", "../other") == .send)
    #expect(claude.command("goal", "tests pass") == .send)
}

@Test("codex sends goal and skills and leaves its TUI commands out")
func codexSendsGoalAndSkillsAndLeavesItsTuiCommandsOut() {
    #expect(codex.command("goal", "pause") == .send)
    #expect(codex.command("my-skill", "") == .send)
    #expect(codex.command("new", "") == .local(.new))
    #expect(codex.command("clear", "") == .unsupported)
    #expect(codex.command("effort", "high") == .local(.effort))
    #expect(codex.command("review", "") == .unsupported)
    #expect(codex.command("mcp", "") == .local(.mcp))
}

@Test("an unknown provider sends every command")
func anUnknownProviderSendsEveryCommand() {
    #expect(Adapter.for("gemini").command("cost", "") == .send)
}

@Test("the menu puts the agent's own first, then its list without panels or twins")
func theMenuPutsTheAgentsOwnFirstThenItsListWithoutPanelsOrTwins() {
    let rows = menu(claude, listed(["goal", "cost", "clear", "review"]), "")
    #expect(names(rows) == ["model", "effort", "compact", "btw", "goal", "mcp", "review"])
    #expect(rows[0].kind == .local(.model))
}

@Test("the menu keeps names that hold the query, those it starts first")
func theMenuKeepsNamesThatHoldTheQueryThoseItStartsFirst() {
    #expect(names(menu(codex, listed(["release-notes", "Rebase"]), "re")) == ["release-notes", "Rebase"])
    #expect(names(menu(codex, listed(["write-note"]), "e")) == ["effort", "model", "new", "write-note"])
}

@Test("codex and unknown providers get an adapter")
func codexAndUnknownProvidersGetAnAdapter() {
    for provider in ["claude", "codex", "gemini"] {
        let can = Adapter.for(provider).capabilities
        #expect(can.interrupt && can.compact && can.sendWhileWorking, "\(provider)")
        #expect(can.subAgents == (provider != "codex"), "\(provider)")
    }
}

@Test("pocketd's chat caps win over the adapter's")
func pocketdsChatCapsWinOverTheAdapters() {
    let said = ChatCaps(interrupt: true, compact: false, sendWhileWorking: false, subAgents: false)
    #expect(capabilities("claude", said) == Capabilities(interrupt: true))
    #expect(capabilities("codex", nil) == Adapter.for("codex").capabilities)
}

@Test("the composer blocks only what the agent can't do")
func theComposerBlocksOnlyWhatTheAgentCantDo() {
    let all = codex.capabilities
    #expect([Action.send, .steer, .queue, .stop].allSatisfy(all.allows))
    #expect([Action.send, .steer, .queue, .stop].map(Capabilities().allows) == [true, false, false, false])
    #expect(!all.allows(.stopping), "a stop under way has nothing to press")
}

private func marked(_ text: String) -> (commands: [String], mentions: [String]) {
    let m = marks(text)
    let u = Array(text.utf8)
    let s = { (r: Range<Int>) in String(decoding: u[r], as: UTF8.self) }
    return (m.commands.map(s), m.mentions.map(s))
}

@Test("a command that opens the text is marked")
func aCommandThatOpensTheTextIsMarked() {
    #expect(marked("  /review 482").commands == ["/review"])
    #expect(marked("/goal tests pass").commands == ["/goal"])
    #expect(marked("/usage/report.md is empty").commands == [])
}

@Test("every skill a prompt names is marked")
func everySkillAPromptNamesIsMarked() {
    #expect(marked("/prototype /apple-design cho mình xem thử").commands == ["/prototype", "/apple-design"])
    #expect(marked("try /figma:figma-use\nthen /review").commands == ["/figma:figma-use", "/review"])
    #expect(marked("@src/chat.rs then /review") == (["/review"], ["@src/chat.rs"]))
}

@Test("a skill keeps its mark before punctuation")
func aSkillKeepsItsMarkBeforePunctuation() {
    #expect(marked("/prototype, /apple-design. why /model? or /review: and /done…").commands == ["/prototype", "/apple-design", "/model", "/review", "/done"])
}

@Test("paths and numbers stay plain")
func pathsAndNumbersStayPlain() {
    #expect(marked("see /usr/bin, /notes.md and and/or, 1 /2").commands == [])
}

@Test("mentions start the text or follow whitespace")
func mentionsStartTheTextOrFollowWhitespace() {
    #expect(marked("@src/chat.rs and\n@a_b-c.md, then @docs/").mentions == ["@src/chat.rs", "@a_b-c.md", "@docs/"])
}

@Test("an MCP tag is no file")
func anMcpTagIsNoFile() {
    #expect(marked("use @mcp/fff and @mcp.json").mentions == ["@mcp.json"])
}

@Test("emails and a lone at stay plain")
func emailsAndALoneAtStayPlain() {
    #expect(marked("mail dev@pocket.app or a/b @ alone").mentions == [])
}

@Test("a mention shows its file's name or its folder's name and slash")
func aMentionShowsItsFilesNameOrItsFoldersNameAndSlash() {
    #expect(mentionName("src/parser.rs") == "parser.rs")
    #expect(mentionName("tsconfig.json") == "tsconfig.json")
    #expect(mentionName("packages/src/") == "src/")
}

private func server(_ name: String, _ status: McpServer.Status) -> McpServer { McpServer(name: name, status: status, scope: "user") }

@Test("the picker lists usable servers first, then those waiting on a sign-in")
func thePickerListsUsableServersFirstThenThoseWaitingOnASignIn() {
    let all = [server("zed", .failed), server("linear", .needsAuth), server("fff", .connected), server("Docs", .pending), server("old", .disabled)]
    #expect(Mcp.picker(all, "").map(\.name) == ["Docs", "fff", "linear", "zed", "old"])
    #expect(Mcp.picker(all, "LIN").map(\.name) == ["linear"])
    #expect(Mcp.picker(all, "user").count == 5, "the scope matches too")
}

@Test("a server row offers what a pick does, else says how it stands")
func aServerRowOffersWhatAPickDoes() {
    #expect(Mcp.label("claude", server("a", .needsAuth), signingIn: false, acting: nil) == ("Sign in", .waiting))
    #expect(Mcp.label("claude", server("a", .needsAuth), signingIn: true, acting: nil) == ("Sign in again", .waiting))
    #expect(Mcp.label("claude", server("a", .needsAuth), signingIn: true, acting: .auth) == ("Signing in…", .muted))
    #expect(Mcp.label("claude", server("a", .failed), signingIn: false, acting: nil) == ("Reconnect", .link))
    #expect(Mcp.label("claude", server("a", .disabled), signingIn: false, acting: nil) == ("Enable", .link))
    #expect(Mcp.label("codex", server("a", .disabled), signingIn: false, acting: nil) == ("Disabled", .dim))
    #expect(Mcp.label("claude", server("a", .connected), signingIn: false, acting: nil) == ("Available", .muted))
    #expect(Mcp.status(server("a", .needsAuth), signingIn: true) == "Finish signing in in your browser")
}

@Test("only a connected or connecting server is usable")
func onlyAConnectedOrConnectingServerIsUsable() {
    #expect(Mcp.usable(server("a", .connected)) && Mcp.usable(server("a", .pending)))
    #expect(!Mcp.usable(server("a", .needsAuth)) && !Mcp.usable(server("a", .failed)) && !Mcp.usable(server("a", .disabled)))
}

@Test("a server the agent can't use is signed in to, reconnected or enabled")
func aServerTheAgentCantUseIsSignedInToReconnectedOrEnabled() {
    #expect(Mcp.action("codex", server("a", .needsAuth)) == .auth)
    #expect(Mcp.action("claude", server("a", .failed)) == .reconnect)
    #expect(Mcp.action("claude", server("a", .disabled)) == .enable)
    #expect(Mcp.action("codex", server("a", .disabled)) == nil, "codex can't enable one from the chat")
    #expect(Mcp.action("claude", server("a", .connected)) == nil)
}

@Test("a tag is a whole word naming a known server")
func aTagIsAWholeWordNamingAKnownServer() {
    let all = [server("fff", .connected), server("docs", .connected)]
    #expect(Mcp.tagged("use @mcp/docs, then @mcp/fff", all).map(\.name) == ["docs", "fff"])
    #expect(Mcp.tagged("use @mcp/fffx", all).isEmpty, "a longer word names another server")
    #expect(Mcp.tagged("mail@mcp/fff", all).isEmpty, "a tag starts a word")
    #expect(Mcp.tagged("@mcp/other", all).isEmpty)
    #expect(Mcp.tagged("(@mcp/fff) and @mcp/fff", all).count == 1, "a server tagged twice counts once")
}

@Test("of two names that start alike, the longer that fits is the tag")
func ofTwoNamesThatStartAlikeTheLongerThatFitsIsTheTag() {
    let all = [server("claude.ai Atlassian", .connected), server("claude.ai Atlassian MCP", .connected)]
    #expect(Mcp.tagged("ask @mcp/claude.ai Atlassian MCP for it", all).map(\.name) == ["claude.ai Atlassian MCP"])
    #expect(Mcp.tagged("ask @mcp/claude.ai Atlassian for it", all).map(\.name) == ["claude.ai Atlassian"])
}

@Test("a prompt that tags servers asks the agent to use them")
func aPromptThatTagsServersAsksTheAgentToUseThem() {
    let all = [server("fff", .connected), server("docs", .connected)]
    #expect(Mcp.withContext("find it with @mcp/fff", all, "codex") == "MCP context: Use the configured server \"fff\" (codex) when relevant to this request.\n\nfind it with @mcp/fff")
    #expect(Mcp.withContext("@mcp/docs and @mcp/fff", all, "claude").hasPrefix("MCP context: Use the configured servers \"docs\" (claude), \"fff\" (claude) when"))
    #expect(Mcp.withContext("no tags here", all, "codex") == "no tags here")
}

private func spawnedAgent(_ id: String, _ status: CallStatus, _ spawns: [Block]) -> Block {
    let transcript = Transcript(turns: [Turn(id: "\(id)-t", blocks: spawns)])
    return .tool(Call(id: id, name: "Agent", kind: .agent(SubAgent(description: id, transcript: transcript)), status: status))
}

private func shellBlock(_ id: String) -> Block { .tool(Call(id: id, name: "Bash", kind: .shell(command: "ls"), status: .ok)) }
private func mapIds(_ map: [Spawned]) -> [String] { map.map { "\($0.call.id):\($0.parent.map(String.init) ?? "-")" } }

@Test("each sub-agent follows the one that spawned it")
func eachSubAgentFollowsTheOneThatSpawnedIt() {
    let t = Transcript(turns: [
        Turn(id: "t", blocks: [spawnedAgent("a", .ok, [shellBlock("s"), spawnedAgent("a1", .ok, [])]), shellBlock("x"), spawnedAgent("b", .running, [])])
    ])
    #expect(mapIds(spawned(t)) == ["a:-", "a1:0", "b:-"])
}

@Test("the map holds the latest turn that spawned and earlier agents still at work")
func theMapHoldsTheLatestTurnThatSpawnedAndEarlierAgentsStillAtWork() {
    let ended = End.done(durationMs: 1, usage: nil)
    let t = Transcript(turns: [
        Turn(id: "t", blocks: [spawnedAgent("old", .ok, []), spawnedAgent("bg", .running, [])], end: ended),
        Turn(id: "t", blocks: [spawnedAgent("new", .failed, [])], end: ended),
        Turn(id: "t", blocks: [shellBlock("s")]),
    ])
    #expect(mapIds(spawned(t)) == ["bg:-", "new:-"])
}

@Test("a conversation that spawned nothing has an empty map")
func aConversationThatSpawnedNothingHasAnEmptyMap() {
    #expect(spawned(Transcript(turns: [Turn(id: "t", blocks: [shellBlock("s")], end: .done(durationMs: 1, usage: nil))])).isEmpty)
}
