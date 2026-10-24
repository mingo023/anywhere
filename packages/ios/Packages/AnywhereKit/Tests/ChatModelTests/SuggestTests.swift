import Testing
import Wire
@testable import ChatModel

private func len(_ s: String) -> Int { s.utf8.count }

@Test("a command is typed from a slash that starts a word to its first space")
func aCommandIsTypedFromASlash() {
    #expect(word("/mo", 3, "/")! == (0, "mo"))
    #expect(word("/", 1, "/")! == (0, ""))
    #expect(word("/model haiku", 12, "/") == nil)
    #expect(word("/model haiku", 3, "/")! == (0, "mo"))
    #expect(word("see /mo", 7, "/")! == (4, "mo"))
    #expect(word("src/main", 8, "/") == nil)
}

@Test("a picked command keeps the argument typed after it")
func aPickedCommandKeepsTheArgument() {
    let (name, arg) = slash("/mo  haiku")!
    #expect(name == "mo")
    #expect(named("model", arg) == ("/model haiku", len("/model ")))
    #expect(named("compact", "") == ("/compact ", len("/compact ")))
}

@Test("a mention starts the draft or follows whitespace")
func aMentionStartsTheDraft() {
    #expect(word("@src", 4, "@")! == (0, "src"))
    #expect(word("look at @src/ma", 15, "@")! == (8, "src/ma"))
    #expect(word("line\n@", 6, "@")! == (5, ""))
    #expect(word("mail me@host", 12, "@") == nil)
    #expect(word("@src done", 9, "@") == nil)
    #expect(word("tiếng @a", len("tiếng @a"), "@")! == (len("tiếng "), "a"))
}

@Test("files the agent touched come first then changed ones then the rest")
func filesTheAgentTouchedComeFirst() {
    let files = ["a.rs", "b.rs", "c.rs", "d.rs", "readme.md"]
    #expect(rank("rs", touched: ["d.rs", "c.rs"], changed: ["b.rs"], files: files) == ["d.rs", "c.rs", "b.rs", "a.rs"])
    #expect(rank("", touched: [], changed: [], files: files).count == files.count)
    #expect(rank("READ", touched: [], changed: [], files: files) == ["readme.md"])
}

@Test("every matching file is ranked so the list scrolls past a window")
func everyMatchingFileIsRanked() {
    let files = (0..<20).map { "f\($0).rs" }
    let ranked = rank("f", touched: [], changed: ["f19.rs"], files: files)
    #expect(ranked.count == 20)
    #expect(Array(ranked.prefix(3)) == ["f19.rs", "f0.rs", "f1.rs"])
}

@Test("commands are asked for each time a slash starts a word and while none came")
func commandsAreAskedFor() {
    var s = Suggest()
    var ask = s.asksCommands("a", "/", 1)
    #expect(ask)
    ask = s.asksCommands("a", "/mo", 3)
    #expect(!ask)
    ask = s.asksCommands("a", "/", 1)
    #expect(ask)
    ask = s.asksCommands("b", "/mo", 3)
    #expect(ask)
    ask = s.asksCommands("a", "ok /", 4)
    #expect(ask)
    ask = s.asksCommands("c", "a/b", 3)
    #expect(!ask)
}

@Test("servers are asked for each time a tag starts and while none came")
func serversAreAskedFor() {
    var s = Suggest()
    var ask = s.asksServers("a", "@mcp/", 5)
    #expect(ask)
    ask = s.asksServers("a", "@mcp/ff", 7)
    #expect(!ask)
    ask = s.asksServers("a", "use @mcp/", 9)
    #expect(ask)
    ask = s.asksServers("b", "use @mcp/ff", 11)
    #expect(ask)
    ask = s.asksServers("c", "@src/main", 9)
    #expect(!ask)
}

@Test("a send asks the agent to use only the servers it told of")
func aSendAsksTheAgentToUseOnlyItsServers() {
    var s = Suggest()
    #expect(s.withMcpContext("a", "codex", "use @mcp/fff") == "use @mcp/fff")
    s.mcpListed("a", .success([McpServer(name: "fff", status: .connected)]), authUrl: "", acted: false)
    #expect(s.withMcpContext("a", "codex", "use @mcp/fff").hasPrefix(#"MCP context: Use the configured server "fff" (codex)"#))
    #expect(s.withMcpContext("b", "codex", "use @mcp/fff") == "use @mcp/fff")
}

@Test("a sign in sent to the browser waits until the server no longer needs one")
func aSignInSentToTheBrowserWaits() {
    let server = { McpServer(name: "linear", status: $0) }
    var s = Suggest()
    let action = s.act("a", "codex", server(.needsAuth))
    #expect(action == .auth)
    s.mcpListed("a", .success([server(.needsAuth)]), authUrl: "https://auth.example", acted: true)
    #expect(s.signsIn("a", "linear") && s.acting.isEmpty)
    s.mcpListed("a", .success([server(.needsAuth)]), authUrl: "", acted: false)
    #expect(s.signsIn("a", "linear"))
    s.mcpListed("a", .success([server(.connected)]), authUrl: "", acted: false)
    #expect(!s.signsIn("a", "linear"))
}

@Test("an agent acts on one server at a time until the actions own reply")
func anAgentActsOnOneServerAtATime() {
    let server = { McpServer(name: $0, status: .needsAuth) }
    var s = Suggest()
    var action = s.act("a", "claude", server("linear"))
    #expect(action == .auth)
    action = s.act("a", "claude", server("asana"))
    #expect(action == nil)
    s.mcpListed("a", .success([server("linear")]), authUrl: "", acted: false)
    action = s.act("a", "claude", server("asana"))
    #expect(action == nil)
    s.mcpListed("a", .success([server("linear")]), authUrl: "https://auth.example", acted: true)
    #expect(s.signsIn("a", "linear"))
    action = s.act("a", "claude", server("asana"))
    #expect(action == .auth)
}

@Test("a sign in that needs no browser waits on nothing")
func aSignInThatNeedsNoBrowser() {
    var s = Suggest()
    s.acting["a"] = ("fff", .auth)
    s.mcpListed("a", .success([]), authUrl: "", acted: true)
    #expect(!s.signsIn("a", "fff") && s.acting.isEmpty)
}

@Test("servers are asked again once at a time only while a sign in or a connection settles")
func serversAreAskedAgain() {
    let server = { McpServer(name: "fff", status: $0) }
    var s = Suggest()
    s.mcpListed("a", .success([server(.connected)]), authUrl: "", acted: false)
    var again = s.rechecks("a")
    #expect(!again)
    s.signingIn.insert(.init(agent: "a", server: "linear"))
    again = s.rechecks("a")
    #expect(again)
    again = s.rechecks("a")
    #expect(!again)
    s.mcpListed("b", .success([server(.pending)]), authUrl: "", acted: false)
    again = s.rechecks("b")
    #expect(again)
}

@Test("files are listed each time an at starts a mention and while none came")
func filesAreListed() {
    var s = Suggest()
    var load = s.loadsFiles("/r", "see @ma", 7)
    #expect(load)
    load = s.loadsFiles("/r", "see @mai", 8)
    #expect(!load)
    load = s.loadsFiles("/r", "see @", 5)
    #expect(load)
    load = s.loadsFiles("/r", "mail me@x", 9)
    #expect(!load)
}

@Test("the selection wraps at either end")
func theSelectionWraps() {
    var s = Suggest()
    s.step(down: false, total: 3)
    #expect(s.selected == 2)
    s.step(down: true, total: 3)
    #expect(s.selected == 0)
    s.selected = 9
    s.step(down: true, total: 3)
    #expect(s.selected == 0)
}

@Test("the window scrolls only to keep the selection in view")
func theWindowScrolls() {
    let n = Suggest.shown
    #expect(window(0, 20) == 0..<n)
    #expect(window(n - 1, 20) == 0..<n)
    #expect(window(n, 20) == 1..<n + 1)
    #expect(window(19, 20) == 20 - n..<20)
    #expect(window(2, 3) == 0..<3)
}

@Test("a picked file replaces the mention and the cursor follows it")
func aPickedFileReplacesTheMention() {
    #expect(replaced("fix @ma please", 4, 7, "@src/main.rs") == ("fix @src/main.rs please", len("fix @src/main.rs ")))
    #expect(replaced("@ma", 0, 3, "@main.rs") == ("@main.rs ", len("@main.rs ")))
}

@Test("a command picked past the drafts start replaces only its word")
func aCommandPickedPastTheStart() {
    #expect(!startsDraft("ok /imp", 3))
    #expect(replaced("ok /imp", 3, 7, "/implement") == ("ok /implement ", len("ok /implement ")))
    #expect(replaced("ok /imp then", 3, 7, "/implement") == ("ok /implement then", len("ok /implement ")))
}

@Test("a slash after only whitespace still starts the draft")
func aSlashAfterOnlyWhitespace() {
    #expect(startsDraft("/mo", 0))
    #expect(startsDraft(" \n/mo", 2))
    #expect(slash(" \n/model")! == ("model", ""))
}

@Test("a slash past the drafts start lists only what can go mid prompt")
func aSlashPastTheStartListsOnlyPrompts() {
    let s = Suggest()
    guard case .commands(let at, _, let rows)? = s.rows("ok /", 4, agent: "a", provider: "claude", cwd: "/r", touched: []) else {
        Issue.record("a slash lists commands")
        return
    }
    #expect(at == 3)
    #expect(rows.allSatisfy { $0.kind == .send || $0.kind == .local(.mcp) })
}

@Test("a mention lists the files under the agents folder the agent touched first")
func aMentionListsFiles() {
    var s = Suggest()
    s.filesListed("/r", ["a.rs", "b.rs"])
    guard case .files(_, _, let paths)? = s.rows("@", 1, agent: "a", provider: "claude", cwd: "/r", touched: ["/r/b.rs", "/x/a.rs"]) else {
        Issue.record("an at lists files")
        return
    }
    #expect(paths == ["b.rs", "a.rs"])
}

private func toolBlock(_ kind: ToolKind) -> Block { .tool(Call(id: "", name: "", kind: kind, status: .ok)) }

private func transcript(_ blocks: [Block]) -> Transcript { Transcript(turns: [Turn(id: "", blocks: blocks)]) }

@Test("touched files come latest first once each with a sub agents")
func touchedFilesComeLatestFirst() {
    let sub = transcript([toolBlock(.edit(path: "/r/b.rs", diff: nil))])
    let t = transcript([
        toolBlock(.read(path: "/r/a.rs")), toolBlock(.agent(SubAgent(description: "", transcript: sub))), toolBlock(.shell(command: "ls")),
        toolBlock(.read(path: "/r/a.rs")), toolBlock(.write(path: "/r/c.rs", diff: nil)),
    ])
    #expect(touched(t) == ["/r/c.rs", "/r/a.rs", "/r/b.rs"])
}

@Test("a bare name means the latest touched file so named before any in the repo")
func aBareNameMeansTheLatestTouched() {
    let touched = ["/r/src/x_add_to_chat.rs", "/r/src/b/add_to_chat.rs", "/r/src/a/add_to_chat.rs"]
    #expect(candidates("add_to_chat.rs", cwd: "/r", touched: touched, repo: { Issue.record("the repo isn't listed"); return [] }) == ["/r/src/b/add_to_chat.rs"])
}

@Test("untouched names are matched whole against the repo")
func untouchedNamesAreMatchedWhole() {
    let repo = { ["src/a/add_to_chat.rs", "src/my_add_to_chat.rs", "b/add_to_chat.rs", "add_to_chat.rs.bak"] }
    #expect(candidates("add_to_chat.rs", cwd: "/r", touched: [], repo: repo) == ["/r/src/a/add_to_chat.rs", "/r/b/add_to_chat.rs"])
    #expect(candidates("a/add_to_chat.rs", cwd: "/r", touched: [], repo: repo) == ["/r/src/a/add_to_chat.rs"])
    #expect(candidates("gone.rs", cwd: "/r", touched: [], repo: repo).isEmpty)
}

@Test("a fenced block reads its language and code")
func aFencedBlockReadsItsLanguage() {
    #expect(fence("```rust\nfn main() {}\n```\n")! == ("rust", "fn main() {}", 0))
    #expect(fence("~~~\na\nb")! == ("", "a\nb", 0))
    #expect(fence("Some *text*") == nil)
}

@Test("a fence under a list item loses its indent from each line")
func aFenceUnderAListItem() {
    #expect(fence("   ```sh\n   brew install\n     --cask x\n  y\n   ```")! == ("sh", "brew install\n  --cask x\ny", 3))
}

@Test("a link opens a web page or a file under the agents folder")
func aLinkOpensAWebPageOrAFile() {
    #expect(target("https://x.dev/a#b", cwd: "/r") == .web("https://x.dev/a#b"))
    #expect(target("src/a.rs", cwd: "/r") == .file("/r/src/a.rs"))
    #expect(target("src/a.rs:42:7", cwd: "/r") == .file("/r/src/a.rs"))
    #expect(target("a.rs#L42", cwd: "/r") == .file("/r/a.rs"))
    #expect(target("file:///etc/hosts", cwd: "/r") == .file("/etc/hosts"))
}

@Test("a prompt over five wrapped lines collapses")
func aPromptOverFiveLinesCollapses() {
    #expect(!Bubble.needsCollapse(5))
    #expect(Bubble.needsCollapse(6))
}

@Test("a bubble shows each mention as its name and keeps its path")
func aBubbleShowsEachMention() {
    let s = Bubble.segments("/review then @src/parser.rs and @docs/")
    #expect(
        s == [
            .init("/review", .command), .init(" then ", .plain), .init("parser.rs", .mention(path: "src/parser.rs")), .init(" and ", .plain),
            .init("docs/", .mention(path: "docs/")),
        ])
}

@Test("a bubble tints each skill where it shows after a mention")
func aBubbleTintsEachSkill() {
    let tinted = Bubble.segments("/prototype @src/parser.rs with /apple-design").filter { $0.kind != .plain }.map(\.text)
    #expect(tinted == ["/prototype", "parser.rs", "/apple-design"])
}

@Test("a bubble eases longer the further it moves up to 850ms")
func aBubbleEasesLonger() {
    #expect(Bubble.tween(0) == 0.22)
    #expect(Bubble.tween(500) == 0.38)
    #expect(Bubble.tween(5000) == 0.85)
}
