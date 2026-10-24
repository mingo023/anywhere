import Client
import Foundation
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
        await pushChecks()
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
        guard let terminal = setup.claudeTerminal else { return skip("terminal.*", "no terminal was started (--claude)") }
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
    }

    func workspaceChecks(_ setup: Setup) async {
        guard let project = setup.project else { return skip("branch.list", "no --project") }
        await check("branch.list", cap: Caps.workspace) {
            let list = try await request(.branchList(.init(tree: project))) { if case .branchList(let l) = $0 { l } else { nil } }
            return "current \(list.current ?? "-"), \(list.local.count) local, \(list.remote.count) remote"
        }
        await check("account.list", cap: Caps.workspace) {
            let list = try await request(.accountList(.init())) { if case .accountList(let l) = $0 { l } else { nil } }
            return "\(list.accounts.count) accounts"
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
    }

    func ghChecks(_ setup: Setup) async {
        await check("gh.status", cap: Caps.gh) {
            let status = try await request(.ghStatus(.init(tree: setup.project))) { if case .ghStatus(let s) = $0 { s } else { nil } }
            return "gh \(status.state)\(status.login.map { " as \($0)" } ?? "")"
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
        await check("fs.search", cap: Caps.fs) {
            let found = try await request(.fsSearch(.init(tree: project, query: "readme"))) { if case .fsSearch(let s) = $0 { s } else { nil } }
            guard found.paths.contains("README.md") else { throw Failure("README.md not found: \(found.paths)") }
            return "\(found.paths.count) paths"
        }
        await check("outside the tree is refused", cap: Caps.fs) {
            do {
                _ = try await connection.request(.fsRead(.init(tree: project, path: "../etc/passwd")))
            } catch DaemonError.server(let error) where error.code == .outsideTree {
                return "outside_tree"
            }
            throw Failure("../etc/passwd wasn't refused as outside_tree")
        }
    }

    func pushChecks() async {
        await check("push.register then push.unregister", cap: Caps.push) {
            let handle = PushHandle(handle: "probe-handle", secret: "probe-secret", expiresAt: Int(Date().timeIntervalSince1970) + 3600)
            let key = Data((0..<32).map { _ in UInt8.random(in: 0...255) }).base64EncodedString()
            let prefs = PushPrefs(needsYou: true, done: true, failed: true, automations: false, details: false)
            _ = try await request(.pushRegister(.init(key: key, env: "development", alert: handle, prefs: prefs))) { $0 == .ack ? () : nil }
            _ = try await request(.pushUnregister) { $0 == .ack ? () : nil }
            return "registered and dropped a device record"
        }
    }
}
