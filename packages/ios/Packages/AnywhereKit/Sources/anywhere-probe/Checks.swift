import Client
import Foundation
import PushSupport
import Stores
import Wire

/// What the checks may use beyond the connection.
struct Setup {
    /// A registered project's path; its checks are skipped without one.
    var project: String?
    /// A file in the project that has uncommitted changes, for git.diff.
    var changedFile: String?
    /// A terminal the owner socket started with a claude in it.
    var claudeTerminal: String?
    /// Where a fake relay logs each /v1/send, one JSON line with whether its signature held.
    var relayLog: String?
    /// The fake gh's folder (`FAKEGH`), whose `calls` logs each gh run.
    var fakeGh: String?
}

extension Probe {
    func runAll(_ setup: Setup) async {
        await helloChecks()
        await check("agent.list") {
            let list = try await request(.agentList) { if case .agentList(let l) = $0 { l } else { nil } }
            return "\(list.agents.count) agents; the store holds \(host.sessions.sessions.count)"
        }
        await check("project.list") {
            let list = try await request(.projectList) { if case .projectList(let l) = $0 { l } else { nil } }
            if let project = setup.project, !list.projects.contains(where: { $0.path == project }) {
                throw Failure("\(project) isn't among \(list.projects.map(\.path))")
            }
            return "\(list.projects.count) projects, \(list.projects.reduce(0) { $0 + $1.worktrees.count }) worktrees"
        }
        let agentId = await claudeChecks(setup)
        await terminalChecks(setup)
        await workspaceChecks(setup)
        await gitChecks(setup)
        await ghChecks(setup)
        await fsChecks(setup)
        await pushChecks(setup)
        if agentId == nil, setup.claudeTerminal == nil { skip("timeline", "no claude was started (--claude)") }
    }

    // MARK: hello

    func helloChecks() async {
        await check("hello") {
            guard let ok = hello else { throw Failure("no hello.ok") }
            guard ok.protocolVersion == 3 else { throw Failure("protocol \(ok.protocolVersion)") }
            let base = [Caps.scopes, Caps.host, Caps.registry, Caps.summaryV2, Caps.launch, Caps.locals]
            let missing = base.filter { !ok.has($0) }
            guard missing.isEmpty else { throw Failure("caps missing: \(missing)") }
            guard Set(ok.scopes ?? []) == [.observe, .drive, .approve, .spawn] else { throw Failure("scopes \(ok.scopes ?? [])") }
            let phone = [Caps.terminal, Caps.workspace, Caps.git, Caps.gh, Caps.fs, Caps.push].filter(ok.has)
            return "\(ok.hostname), pocketd \(ok.version ?? "?"), \(ok.caps?.count ?? 0) caps; phone caps: \(phone.isEmpty ? "none yet" : phone.joined(separator: " "))"
        }
        await check("host state reaches the store", cap: Caps.host) {
            guard let state = host.hostState else { throw Failure("no host state") }
            return "tailnet \(state.tailnet), keeping awake \(state.keepingAwake)"
        }
    }

    // MARK: agents and timeline

    /// Waits for the started claude to show up attached, prompts it, and reads its reply back from the timeline.
    func claudeChecks(_ setup: Setup) async -> String? {
        guard let terminal = setup.claudeTerminal else { return nil }
        var agentId: String?
        await check("a started claude reaches the sessions store, attached") {
            try await until("the claude's agent", seconds: 15) {
                agentId = host.sessions.sessions.values.first { $0.summary.terminalId == terminal && $0.summary.attached }?.id
                return agentId != nil
            }
            return "agent \(agentId!) in terminal \(terminal); Up next \(host.sessions.upNext.count)"
        }
        guard let agentId else { return nil }
        let text = "probe ping \(Int.random(in: 1000...9999))"
        await check("agent.prompt then agent.timeline holds the reply") {
            let mark = events.count
            _ = try await request(.agentView(.init(agentIds: [agentId]))) { $0 == .ack ? () : nil }
            _ = try await request(.agentPrompt(.init(agentId: agentId, text: text))) { $0 == .ack ? () : nil }
            let deadline = Date().addingTimeInterval(15)
            while true {
                let timeline = try await self.timeline(agentId)
                let replied = timeline.items.contains { item in
                    if case .assistant(let reply) = item.body { reply.contains("echo: \(text)") } else { false }
                }
                if replied {
                    let streamed = events[mark...].filter {
                        if case .message(.agentStream(let s)) = $0 { s.agentId == agentId } else { false }
                    }.count
                    return "\(timeline.items.count) items, maxSeq \(timeline.maxSeq), \(streamed) streamed while waiting"
                }
                guard Date() < deadline else { throw Failure("no \"echo: \(text)\" in \(timeline.items.count) items") }
                try await Task.sleep(for: .milliseconds(200))
            }
        }
        await check("agent.timeline beforeSeq pages back with an exact hasOlder") {
            let full = try await timeline(agentId)
            guard full.items.count >= 2, let first = full.items.first?.seq else { throw Failure("only \(full.items.count) items") }
            let page = try await timeline(agentId, beforeSeq: full.maxSeq, limit: 1)
            guard page.items.count == 1, let seq = page.items.first?.seq, seq < full.maxSeq else { throw Failure("page \(page.items.map(\.seq))") }
            guard page.hasOlder == (seq > first) else { throw Failure("hasOlder \(page.hasOlder) for seq \(seq), oldest \(first)") }
            let oldest = try await timeline(agentId, beforeSeq: first + 1, limit: 5)
            guard oldest.hasOlder == false else { throw Failure("hasOlder true past the oldest item") }
            return "seq \(seq) of \(full.maxSeq), hasOlder \(page.hasOlder)"
        }
        return agentId
    }

    func timeline(_ agentId: String, beforeSeq: Int? = nil, limit: Int? = nil) async throws -> AgentTimeline {
        try await request(.agentTimeline(.init(agentId: agentId, beforeSeq: beforeSeq, limit: limit))) {
            if case .agentTimeline(let t) = $0 { t } else { nil }
        }
    }

    // MARK: other slices (each runs once its cap is offered)

    func terminalChecks(_ setup: Setup) async {
        if let terminal = setup.claudeTerminal {
            await check("terminal.list", cap: Caps.terminal) {
                let list = try await request(.terminalList) { if case .terminalList(let l) = $0 { l } else { nil } }
                guard list.terminals.contains(where: { $0.id == terminal }) else { throw Failure("\(terminal) not listed") }
                return "\(list.terminals.count) terminals"
            }
            await check("terminal.attach sends a snapshot, and input echoes as output", cap: Caps.terminal) {
                let mark = events.count
                let attached = try await request(.terminalAttach(.init(terminalId: terminal))) {
                    if case .terminalAttached(let a) = $0 { a } else { nil }
                }
                let snapshot = try await event(after: mark, "a SNAPSHOT frame") {
                    if case .terminal(let f) = $0, f.terminalId == terminal, f.kind == .snapshot { f } else { nil }
                }
                let typed = events.count
                try await connection.send(.terminalInput(.init(terminalId: terminal, data: Data("probe input\r".utf8))))
                let output = try await event(after: typed, "an OUTPUT frame after input") {
                    if case .terminal(let f) = $0, f.terminalId == terminal, f.kind == .output { f } else { nil }
                }
                _ = try await request(.terminalDetach(.init(terminalId: terminal))) { $0 == .ack ? () : nil }
                return "\(attached.cols)×\(attached.rows), snapshot \(snapshot.payload.count) B, output \(output.payload.count) B"
            }
        } else {
            skip("terminal.list, terminal.attach", "no terminal was started (--claude)")
        }
        guard let project = setup.project else { return skip("terminal.create", "no --project") }
        await check("terminal.create opens a shell, resize fits it, terminal.close ends it (TerminalStore)", cap: Caps.terminal) {
            let terminals = host.terminals
            terminals.refresh()
            let id = try await terminals.create(cwd: project, localId: nil, cols: 80, rows: 24)
            try await until("the new terminal in the store's list") { terminals.terminal(id) != nil }
            // Only attached screens hear terminal.resized; the list read after it shows the new size.
            terminals.resize(id, cols: 100, rows: 30)
            terminals.refresh()
            try await until("the list showing 100×30") { terminals.terminal(id).map { $0.cols == 100 && $0.rows == 30 } == true }
            try await terminals.close(id)
            try await until("the closed terminal gone from the store") { terminals.terminal(id) == nil }
            return "terminal \(id.prefix(8)) opened in the project, resized to 100×30, closed"
        }
    }

    func workspaceChecks(_ setup: Setup) async {
        guard let project = setup.project else { return skip("workspace.*", "no --project") }
        let workspace = host.workspace
        await check("branch.list", cap: Caps.workspace) {
            let list = try await workspace.loadBranches(tree: project)
            return "current \(list.current ?? "-"), \(list.local.count) local, \(list.remote.count) remote"
        }
        await check("account.list", cap: Caps.workspace) {
            let list = try await request(.accountList(.init())) { if case .accountList(let l) = $0 { l } else { nil } }
            return "\(list.accounts.count) accounts"
        }
        await check("worktree.create streams progress, then worktree.delete takes it and its branch (WorkspaceStore)", cap: Caps.workspace) {
            let name = "probe-\(Int.random(in: 1000...9999))"
            let requestId = UUID().uuidString
            let made = try await workspace.createWorktree(.init(requestId: requestId, project: project, name: name))
            let steps = workspace.creations[requestId]?.steps.map(\.step) ?? []
            guard FileManager.default.fileExists(atPath: made.path) else { throw Failure("\(made.path) wasn't made") }
            try await until("the worktree in the project list") {
                workspace.project(project)?.worktrees.contains { $0.path == made.path } == true
            }
            do {
                try await workspace.deleteWorktree(path: made.path, deleteBranch: true)
            } catch {
                throw Failure("deleting \(made.path): \(error)")
            }
            guard !FileManager.default.fileExists(atPath: made.path) else { throw Failure("\(made.path) is still there") }
            guard try git(project, "branch", "--list", made.branch).isEmpty else { throw Failure("branch \(made.branch) is still there") }
            return "\(made.branch) at \(made.path), steps \(steps.joined(separator: " ")); deleted with its branch"
        }
        await check("local.create, local.rename and local.delete (WorkspaceStore)", cap: Caps.workspace) {
            let local = try await workspace.createLocal(project: project)
            try await workspace.renameLocal(id: local.id, title: "Probe scratch")
            try await until("the renamed Local in the store") { workspace.locals.contains { $0.id == local.id && $0.name == "Probe scratch" } }
            try await workspace.deleteLocal(id: local.id)
            try await until("the Local gone") { !workspace.locals.contains { $0.id == local.id } }
            return "Local \(local.id.prefix(8)) made, renamed and deleted"
        }
    }

    func gitChecks(_ setup: Setup) async {
        guard let project = setup.project else { return skip("git.*", "no --project") }
        await check("git.status", cap: Caps.git) {
            let status = try await request(.gitStatus(.init(tree: project))) { if case .gitStatus(let s) = $0 { s } else { nil } }
            if let file = setup.changedFile, !status.files.contains(where: { $0.path == file }) { throw Failure("\(file) not among the changes") }
            return "branch \(status.branch ?? "-"), \(status.totals.files) changed files"
        }
        if let file = setup.changedFile {
            await check("git.diff", cap: Caps.git) {
                let diff = try await request(.gitDiff(.init(tree: project, path: file, at: .working))) { if case .gitDiff(let d) = $0 { d } else { nil } }
                guard diff.added + diff.removed > 0 else { throw Failure("an empty diff of \(file)") }
                return "\(file): +\(diff.added) −\(diff.removed), \(diff.lines.count) rows, \(diff.language)"
            }
        }
        await check("git.log", cap: Caps.git) {
            let log = try await request(.gitLog(.init(tree: project))) { if case .gitLog(let l) = $0 { l } else { nil } }
            guard !log.commits.isEmpty else { throw Failure("no commits") }
            return "\(log.commits.count) commits, head \(log.tips.head?.prefix(8) ?? "-")"
        }
        let store = host.git
        let changes = store.changes(project)
        await check("git.watch, then a file written on the Mac comes back as git.changed (GitStore)", cap: Caps.git) {
            store.show(project)
            try await until("the tree's first status") { changes.head != nil }
            let mark = events.count
            try "made by the probe\n".write(toFile: project + "/probe.txt", atomically: true, encoding: .utf8)
            let changed = try await push(after: mark, "git.changed", seconds: 15) {
                if case .gitChanged(let c) = $0, c.tree == project { c } else { nil }
            }
            try await until("probe.txt in the store's changes") { changes.files.contains { $0.path == "probe.txt" } }
            return "rev \(changed.rev.prefix(8)); \(changes.files.count) changed files in the store"
        }
        await check("git.stage, git.commit.create, git.commit and git.push (GitStore)", cap: Caps.git) {
            await store.stage(project, paths: ["probe.txt"], staged: true)
            try await until("probe.txt staged") { changes.files.contains { $0.path == "probe.txt" && $0.staged } }
            guard await store.commit(project, message: "Probe commit", kind: .push, all: false) else {
                throw Failure("commit and push failed: \(changes.error ?? "-")")
            }
            let head = try git(project, "rev-parse", "HEAD")
            let detail = try await store.commit(project, sha: head)
            guard detail.files.contains(where: { $0.path == "probe.txt" }) else { throw Failure("the commit doesn't hold probe.txt") }
            guard try git(project, "rev-parse", "origin/main") == head else { throw Failure("origin/main isn't at \(head)") }
            return "\(head.prefix(8)) \"\(detail.subject)\" pushed to origin"
        }
        await check("git.pull and git.discard (GitStore)", cap: Caps.git) {
            guard await store.pull(project) else { throw Failure("pull failed: \(changes.error ?? "-")") }
            try "junk\n".write(toFile: project + "/junk.txt", atomically: true, encoding: .utf8)
            await store.refresh(project)
            await store.discard(project, paths: ["junk.txt"])
            guard !FileManager.default.fileExists(atPath: project + "/junk.txt") else { throw Failure("junk.txt is still there") }
            return "pulled (up to date), junk.txt discarded"
        }
        store.hide(project)
    }

    func ghChecks(_ setup: Setup) async {
        await check("gh.status", cap: Caps.gh) {
            let status = try await request(.ghStatus(.init(tree: setup.project))) { if case .ghStatus(let s) = $0 { s } else { nil } }
            if setup.fakeGh != nil, status.state != .ready { throw Failure("the fake gh reads as \(status.state)") }
            return "gh \(status.state)\(status.login.map { " as \($0)" } ?? "")"
        }
        guard let project = setup.project, let fake = setup.fakeGh else { return skip("gh.pr and its actions", "no fake gh (FAKEGH)") }
        let github = host.github
        let model = github.pullRequest(project)
        await check("gh.pr reads the PR, its checks and threads (GitHubStore)", cap: Caps.gh) {
            await github.refresh(project)
            guard let pr = model.pr else { throw Failure("no PR: \(model.error ?? model.problem?.rawValue ?? "-")") }
            return "#\(pr.number) \(pr.state), \(pr.checks.passed + pr.checks.failed + pr.checks.pending) checks, \(pr.threads.count) threads"
        }
        await check("gh.thread.reply, gh.thread.resolve and gh.pr.ready reach gh (GitHubStore)", cap: Caps.gh) {
            guard let thread = model.pr?.threads.first?.id else { throw Failure("no thread to answer") }
            await github.reply(project, thread: thread, body: "Probe reply")
            await github.resolve(project, thread: thread, resolved: true, undoable: false)
            await github.markReady(project)
            if let error = model.error { throw Failure(error) }
            let calls = (try? String(contentsOfFile: fake + "/calls", encoding: .utf8)) ?? ""
            let graphql = calls.split(separator: "\n").filter { $0.contains("api graphql") && !$0.contains("reviewThreads") }.count
            guard graphql >= 2, calls.contains("pr ready") else { throw Failure("gh ran: \(calls.suffix(400))") }
            return "\(graphql) thread mutations and pr ready ran through gh"
        }
        await check("gh.prs lists the project's PRs (GitHubStore)", cap: Caps.gh) {
            try await github.refreshList(project)
            return "\(github.lists[project]?.count ?? 0) PRs"
        }
    }

    func fsChecks(_ setup: Setup) async {
        guard let project = setup.project else { return skip("fs.*", "no --project") }
        await check("fs.list", cap: Caps.fs) {
            let list = try await request(.fsList(.init(tree: project, dir: ""))) { if case .fsList(let l) = $0 { l } else { nil } }
            guard list.entries.contains(where: { $0.name == "README.md" }) else { throw Failure("no README.md at the root") }
            return "\(list.entries.count) entries at the root"
        }
        await check("fs.read", cap: Caps.fs) {
            let read = try await request(.fsRead(.init(tree: project, path: "README.md"))) { if case .fsRead(let r) = $0 { r } else { nil } }
            guard read.kind == .text, read.text?.isEmpty == false else { throw Failure("README.md read as \(read.kind)") }
            return "README.md, \(read.size) B, \(read.language ?? "no language")"
        }
        await check("fs.search answers the composer's @ mention (FilesStore as ChatFiles)", cap: Caps.fs) {
            let files: any ChatFiles = host.files
            let found = try await files.search(tree: project, query: "readme")
            guard found.contains("README.md") else { throw Failure("README.md not found: \(found)") }
            return "\(found.count) paths"
        }
        await check("outside the tree is refused", cap: Caps.fs) {
            do {
                _ = try await connection.request(.fsRead(.init(tree: project, path: "../etc/passwd")))
            } catch DaemonError.server(let error) where error.code == .outsideTree {
                return "outside_tree"
            }
            throw Failure("../etc/passwd wasn't refused as outside_tree")
        }
        await check("attach.put lands a chat image on the Mac in chunks (FilesStore as ChatFiles)", cap: Caps.fs) {
            // A PNG signature and 1.5 chunks of filler: pocketd checks the first bytes and takes chunks in order.
            var png = Data([0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A])
            png.append(Data((0..<(Limits.attachChunk * 3 / 2)).map { UInt8($0 & 0xff) }))
            let files: any ChatFiles = host.files
            let path = try await files.upload(png, name: "probe.png")
            guard FileManager.default.contents(atPath: path) == png else { throw Failure("\(path) doesn't hold what was sent") }
            return "\(png.count) B at \(path)"
        }
    }

    func pushChecks(_ setup: Setup) async {
        guard let relayLog = setup.relayLog else {
            return await check("push.register then push.unregister", cap: Caps.push) {
                let handle = PushHandle(handle: "probe-handle", secret: "probe-secret", expiresAt: Int(Date().timeIntervalSince1970) + 3600)
                let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) }).base64EncodedString()
                let prefs = PushPrefs(needsYou: true, done: true, failed: true, automations: false, details: false)
                _ = try await request(.pushRegister(.init(key: key, env: "development", alert: handle, prefs: prefs))) { $0 == .ack ? () : nil }
                _ = try await request(.pushUnregister) { $0 == .ack ? () : nil }
                return "registered and dropped a device record"
            }
        }
        await check("push.register, push.prefs, then push.test reaches the relay signed and opens to the Mac's notice", cap: Caps.push) {
            guard let mac = hello?.serverId else { throw Failure("no serverId") }
            let key = NoticeBox.newKey()
            let secret = Data((0..<32).map { _ in UInt8.random(in: 0...255) }).base64EncodedString()
                .replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_").replacingOccurrences(of: "=", with: "")
            // The fake relay reads the secret from the handle; the real one unseals it.
            let handle = PushHandle(handle: "probe.\(secret)", secret: secret, expiresAt: Int(Date().timeIntervalSince1970) + 3600)
            let prefs = PushPrefs(needsYou: true, done: true, failed: true, automations: false, details: false)
            let raw = key.withUnsafeBytes { Data($0) }.base64EncodedString()
            _ = try await request(.pushRegister(.init(key: raw, env: "development", alert: handle, prefs: prefs))) { $0 == .ack ? () : nil }
            _ = try await request(.pushPrefs(.init(prefs: PushPrefs(needsYou: true, done: false, failed: true, automations: false, details: false)))) {
                $0 == .ack ? () : nil
            }
            _ = try await request(.pushTest, timeout: .seconds(35)) { $0 == .ack ? () : nil }
            let sent = try relaySends(relayLog).last { $0.handle == handle.handle }
            guard let sent else { throw Failure("the relay saw no send for this handle") }
            guard sent.signed else { throw Failure("the relay's signature check failed") }
            guard let notice = NoticeBox.open(sent.e, keys: [mac: key]) else { throw Failure("the payload didn't open with the phone's key") }
            guard notice.kind == "test" else { throw Failure("kind \(notice.kind)") }
            _ = try await request(.pushUnregister) { $0 == .ack ? () : nil }
            return "\"\(notice.line)\" at priority \(sent.priority), then unregistered"
        }
    }

    /// The fake relay's log: each send's handle, whether its signature held, priority and sealed `e`.
    func relaySends(_ path: String) throws -> [(handle: String, signed: Bool, priority: Int, e: String)] {
        struct Line: Decodable {
            struct Body: Decodable {
                struct Payload: Decodable { let e: String }
                let handle: String
                let priority: Int
                let payload: Payload
            }
            let signed: Bool
            let body: Body
        }
        let text = (try? String(contentsOfFile: path, encoding: .utf8)) ?? ""
        return try text.split(separator: "\n").map {
            let line = try JSONDecoder().decode(Line.self, from: Data($0.utf8))
            return (line.body.handle, line.signed, line.body.priority, line.body.payload.e)
        }
    }

    /// Runs git in `dir` on this Mac, for checks that look past the protocol at the repo itself.
    func git(_ dir: String, _ args: String...) throws -> String {
        let p = Process()
        p.executableURL = URL(fileURLWithPath: "/usr/bin/git")
        p.arguments = ["-C", dir] + args
        let out = Pipe()
        p.standardOutput = out
        p.standardError = Pipe()
        try p.run()
        let data = out.fileHandleForReading.readDataToEndOfFile()
        p.waitUntilExit()
        guard p.terminationStatus == 0 else { throw Failure("git \(args.joined(separator: " ")) exited \(p.terminationStatus)") }
        return String(decoding: data, as: UTF8.self).trimmingCharacters(in: .whitespacesAndNewlines)
    }
}
