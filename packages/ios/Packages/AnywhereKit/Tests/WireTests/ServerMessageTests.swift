import Foundation
import Testing
@testable import Wire

private func decode(_ json: String) throws -> ServerFrame {
    try ServerFrame.decode(Data(json.utf8))
}

@Suite("Server messages")
struct ServerMessageTests {
    @Test("a message type from a newer pocketd decodes as unknown instead of failing")
    func unknownType() throws {
        let frame = try decode(#"{"type":"agent.sparkle","id":"x1","glitter":[1,2]}"#)
        #expect(frame.message == .unknown("agent.sparkle"))
        #expect(frame.id == "x1")
    }

    @Test("a reply carries the id of the request it answers, a push carries none")
    func replyIds() throws {
        #expect(try decode(#"{"type":"ack","id":"c7"}"#).id == "c7")
        #expect(try decode(#"{"type":"agent.list","agents":[]}"#).id == nil)
    }

    @Test("a known message missing a required field is rejected")
    func missingField() {
        #expect(throws: DecodingError.self) { try decode(#"{"type":"agent.update"}"#) }
        #expect(throws: DecodingError.self) { try decode(#"{"type":"git.changed","tree":"/w"}"#) }
        #expect(throws: DecodingError.self) { try decode(#"{"type":"terminal.attached","id":"a","terminalId":"t1","cols":"80","rows":24}"#) }
    }

    @Test("a frame without a type, or that isn't an object, is rejected")
    func noType() {
        #expect(throws: DecodingError.self) { try decode(#"{"id":"c1"}"#) }
        #expect(throws: DecodingError.self) { try decode(#"[1]"#) }
        #expect(throws: DecodingError.self) { try decode(#"<html>"#) }
    }

    @Test("an agent status a newer pocketd adds still decodes")
    func openStatus() throws {
        let json = #"{"type":"agent.update","agent":{"id":"a1","terminalId":"t1","title":"x","cwd":"/w","provider":"claude","status":"dreaming","attached":true,"epoch":1,"maxSeq":0,"createdAt":1,"updatedAt":2}}"#
        guard case .agentUpdate(let update) = try decode(json).message else { Issue.record("not an update"); return }
        #expect(update.agent.status.rawValue == "dreaming")
    }

    @Test("a timeline item or tool of a kind this build doesn't know keeps the rest of the timeline")
    func unknownKinds() throws {
        let json = #"""
        {"type":"agent.timeline","id":"t","agentId":"a1","epoch":1,"hasOlder":false,"maxSeq":3,"items":[
          {"kind":"hologram","id":"i1","seq":1,"ts":1,"beam":true},
          {"kind":"tool","id":"i2","seq":2,"ts":2,"call":{"toolUseId":"u","name":"Warp","detail":{"kind":"warp","speed":9},"status":"ok"}},
          {"kind":"user","id":"i3","seq":3,"ts":3,"text":"hi"}]}
        """#
        guard case .agentTimeline(let timeline) = try decode(json).message else { Issue.record("not a timeline"); return }
        #expect(timeline.items.map(\.body.kind) == ["hologram", "tool", "user"])
        guard case .tool(let call) = timeline.items[1].body else { Issue.record("not a tool"); return }
        #expect(call.detail == .unknown(kind: "warp"))
    }

    @Test("a failed host names why, and an online one its version and uptime")
    func hostLinks() throws {
        let failed = #"{"type":"host.update","host":{"id":"h","name":"h","target":{"kind":"alias","name":"h"},"socket":"/s","link":{"state":"failed","failure":"auth"}}}"#
        guard case .hostUpdate(let update) = try decode(failed).message else { Issue.record("not an update"); return }
        #expect(update.host.target == .alias(name: "h"))
        #expect(update.host.link == HostLinkState(state: .failed, failure: .auth))
        let online = #"{"type":"host.list","id":"l1","hosts":[{"id":"h","name":"h","target":{"kind":"address","addr":"10.0.4.12","port":22},"socket":"/s","link":{"state":"online","version":"1.4.2","uptime":0}}]}"#
        guard case .hostList(let list) = try decode(online).message else { Issue.record("not a list"); return }
        #expect(list.hosts[0].link == HostLinkState(state: .online, version: "1.4.2", uptime: 0))
    }

    @Test("a host_link item and a remote delegation carry their host")
    func hostLinkAndEnvironment() throws {
        let item = #"{"type":"agent.stream","agentId":"a1","epoch":1,"item":{"kind":"host_link","id":"i","seq":1,"ts":1,"hint":"vps"}}"#
        guard case .agentStream(let stream) = try decode(item).message else { Issue.record("not a stream"); return }
        #expect(stream.item.body == .hostLink(HostLink(hint: "vps")))
        let update = #"{"type":"delegation.update","agentId":"a1","delegation":{"taskId":"t","parentId":"a1","childId":"devbox/a2","title":"x","agent":"codex","status":"working","createdAt":1,"environmentId":"devbox"}}"#
        guard case .delegationUpdate(let delegation) = try decode(update).message else { Issue.record("not an update"); return }
        #expect(delegation.delegation.environmentId == "devbox")
    }

    @Test("an other tool keeps its input as any JSON")
    func otherInput() throws {
        let json = #"{"type":"agent.stream","agentId":"a1","epoch":1,"item":{"kind":"tool","id":"i","seq":1,"ts":1,"call":{"toolUseId":"u","name":"mcp__x","detail":{"kind":"other","name":"mcp__x","input":{"q":[1,"two",null,true,{"n":2.5}]}},"status":"running"}}}"#
        guard case .agentStream(let stream) = try decode(json).message, case .tool(let call) = stream.item.body,
            case .other(_, let input, _) = call.detail
        else { Issue.record("not an other tool"); return }
        #expect(input["q"] == .array([.number(1), .string("two"), .null, .bool(true), .object(["n": .number(2.5)])]))
    }

    @Test("a diff names what it compares")
    func diffAt() throws {
        let cases: [(String, DiffAt)] = [
            (#"{"kind":"working"}"#, .working), (#"{"kind":"staged"}"#, .staged),
            (#"{"kind":"commit","rev":"abc"}"#, .commit(rev: "abc")), (#"{"kind":"base"}"#, .base(rev: nil)),
        ]
        for (json, at) in cases {
            #expect(try JSONDecoder().decode(DiffAt.self, from: Data(json.utf8)) == at)
        }
        #expect(throws: DecodingError.self) { try JSONDecoder().decode(DiffAt.self, from: Data(#"{"kind":"commit"}"#.utf8)) }
        #expect(throws: DecodingError.self) { try JSONDecoder().decode(DiffAt.self, from: Data(#"{"kind":"stash"}"#.utf8)) }
    }

    @Test("launch progress isn't the end of a request, its created reply is")
    func interim() throws {
        #expect(try decode(#"{"type":"agent.progress","id":"c","requestId":"r","step":"fetch"}"#).message.isInterim)
        #expect(try decode(#"{"type":"worktree.progress","id":"c","requestId":"r","step":"setup"}"#).message.isInterim)
        #expect(try !decode(#"{"type":"agent.created","id":"c","requestId":"r","agentId":"a","terminalId":"t"}"#).message.isInterim)
    }

    @Test("an agent that never asks before a tool says so, and one that asks says nothing")
    func neverAsks() throws {
        let json = #"{"type":"agent.providers","id":"p","maxAccess":"full","phoneMaxAccess":"full","providers":["#
            + #"{"id":"pi","available":true,"efforts":[],"plan":true,"neverAsks":true},{"id":"claude","available":true,"efforts":[],"plan":true}]}"#
        let frame = try decode(json)
        guard case .agentProviders(let p) = frame.message else { Issue.record("not providers"); return }
        #expect(p.providers.map(\.neverAsks) == [true, nil])
        #expect(try JSONDecoder().decode(JSONValue.self, from: frame.encoded()) == JSONDecoder().decode(JSONValue.self, from: Data(json.utf8)))
    }

    @Test("a hello.ok from a Mac whose newest protocol is older than the phone's is too old")
    func serverTooOld() {
        #expect(HelloOk(serverId: "m", hostname: "m", protocolVersion: 2, protocol: .init(min: 1, max: 2)).serverTooOld)
        #expect(HelloOk(serverId: "m", hostname: "m", protocolVersion: 2).serverTooOld)
        #expect(!HelloOk(serverId: "m", hostname: "m", protocolVersion: 3, protocol: .init(min: 3, max: 3)).serverTooOld)
    }

    @Test("a project frame decodes the id pocketd gives it, and one without still decodes")
    func projectId() throws {
        let project = #"{"path":"/p","name":"p","worktrees":[]}"#
        let with = try decode(#"{"type":"project.list","projects":[{"id":"p1","path":"/p","name":"p","worktrees":[]}]}"#)
        let without = try decode(#"{"type":"project.list","projects":["# + project + "]}")
        guard case .projectList(let a) = with.message, case .projectList(let b) = without.message else { Issue.record("not project lists"); return }
        #expect(a.projects.map(\.projectId) == ["p1"])
        #expect(a.projects.map(\.id) == ["/p"])
        #expect(b.projects.map(\.projectId) == [nil])
    }

    @Test("an agent summary decodes the project id and tree it started in")
    func agentProject() throws {
        let json = #"{"id":"a","terminalId":"t","title":"x","cwd":"/p","provider":"claude","status":"idle","attached":true,"epoch":1,"maxSeq":0,"createdAt":1,"updatedAt":1"#
        let with = try JSONDecoder().decode(AgentSummary.self, from: Data((json + #","projectId":"p1","tree":"/p/wt"}"#).utf8))
        let without = try JSONDecoder().decode(AgentSummary.self, from: Data((json + "}").utf8))
        #expect([with.projectId, with.tree] == ["p1", "/p/wt"])
        #expect(without.projectId == nil && without.tree == nil)
    }
}
