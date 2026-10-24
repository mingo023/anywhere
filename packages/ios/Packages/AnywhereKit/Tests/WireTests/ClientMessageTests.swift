import Foundation
import Testing
@testable import Wire

private func json(_ frame: ClientFrame) throws -> JSONValue {
    try JSONDecoder().decode(JSONValue.self, from: frame.encoded())
}

@Suite("Client messages")
struct ClientMessageTests {
    @Test("the phone's hello asks for every cap it speaks and the protocol range")
    func hello() throws {
        let frame = ClientFrame(id: "h1", .hello(.init(token: "t", clientId: "ios-1")))
        let value = try json(frame)
        #expect(value["type"] == .string("hello"))
        #expect(value["protocolVersion"] == .number(3))
        #expect(value["protocol"] == .object(["min": .number(3), "max": .number(3)]))
        guard case .array(let caps) = value["caps"] else { Issue.record("no caps"); return }
        #expect(caps.compactMap(\.string) == Caps.phone)
        for cap in ["terminal.v1", "workspace.v1", "git.v1", "gh.v1", "fs.v1", "push.v1", "subagents.v1"] {
            #expect(Caps.phone.contains(cap))
        }
    }

    @Test("optional fields are left out, never sent as null")
    func noNulls() throws {
        let frame = ClientFrame(id: "t1", .agentTimeline(.init(agentId: "a1", beforeSeq: 40, limit: 100)))
        #expect(try json(frame) == .object([
            "type": .string("agent.timeline"), "id": .string("t1"), "agentId": .string("a1"),
            "beforeSeq": .number(40), "limit": .number(100),
        ]))
    }

    @Test("terminal input and attachments travel as base64")
    func base64() throws {
        let input = try json(ClientFrame(id: "i", .terminalInput(.init(terminalId: "t1", data: Data("ls\r".utf8)))))
        #expect(input["data"] == .string("bHMN"))
        let put = try json(ClientFrame(id: "p", .attachPut(.init(uploadId: "u", name: "a.png", index: 0, data: Data([0, 255]), last: true))))
        #expect(put["data"] == .string("AP8="))
    }

    @Test("a verb without fields encodes as just its type and id")
    func bare() throws {
        #expect(try json(ClientFrame(id: "l1", .agentList)) == .object(["type": .string("agent.list"), "id": .string("l1")]))
        #expect(try json(ClientFrame(id: "p", .pushTest)) == .object(["type": .string("push.test"), "id": .string("p")]))
    }

    @Test("a client frame without an id or with an unknown type is rejected")
    func rejects() {
        #expect(throws: DecodingError.self) { try ClientFrame.decode(Data(#"{"type":"agent.list"}"#.utf8)) }
        #expect(throws: DecodingError.self) { try ClientFrame.decode(Data(#"{"type":"exec","id":"x"}"#.utf8)) }
    }

    @Test("a local id is 1 to 64 letters, digits, underscores and dashes")
    func localIds() {
        #expect(Local.isValidId("l1"))
        #expect(Local.isValidId("Local_2-b"))
        #expect(!Local.isValidId(""))
        #expect(!Local.isValidId("/Users/me"))
        #expect(!Local.isValidId(String(repeating: "a", count: 65)))
    }
}
