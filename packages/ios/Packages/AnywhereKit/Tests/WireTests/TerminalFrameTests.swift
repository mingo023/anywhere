import Foundation
import Testing
@testable import Wire

@Suite("Terminal frames")
struct TerminalFrameTests {
    @Test("a frame gives its kind, terminal id and payload")
    func parses() throws {
        let frame = try TerminalFrame(parsing: Data([2, 2]) + Data("t7".utf8) + Data("\u{1b}[1mhi".utf8))
        #expect(frame.kind == .output)
        #expect(frame.terminalId == "t7")
        #expect(frame.payload == Data("\u{1b}[1mhi".utf8))
    }

    @Test("a snapshot can be empty and still names its terminal")
    func emptySnapshot() throws {
        let frame = try TerminalFrame(parsing: Data([1, 3]) + Data("abc".utf8))
        #expect(frame == TerminalFrame(kind: .snapshot, terminalId: "abc", payload: Data()))
    }

    @Test("a frame encodes to the bytes it parses from")
    func roundTrip() throws {
        let frame = TerminalFrame(kind: .output, terminalId: "term-42", payload: Data((0...255).map { UInt8($0) }))
        #expect(try TerminalFrame(parsing: frame.encoded) == frame)
    }

    @Test("a slice of a larger buffer parses the same")
    func slice() throws {
        let buffer = Data([9, 9, 2, 1]) + Data("x".utf8) + Data("y".utf8)
        let frame = try TerminalFrame(parsing: buffer.dropFirst(2))
        #expect(frame.terminalId == "x")
        #expect(frame.payload == Data("y".utf8))
    }

    @Test("an id may hold any printable ASCII, space included, as pocketd allows")
    func printableId() throws {
        let frame = try TerminalFrame(parsing: Data([2, 3]) + Data("a b".utf8))
        #expect(frame.terminalId == "a b")
        #expect(throws: TerminalFrame.ParseError.badId) { try TerminalFrame(parsing: Data([2, 1, 0x1f])) }
        #expect(throws: TerminalFrame.ParseError.badId) { try TerminalFrame(parsing: Data([2, 1, 0x7f])) }
    }

    @Test("short frames, unknown kinds and bad ids are rejected")
    func rejects() {
        #expect(throws: TerminalFrame.ParseError.tooShort) { try TerminalFrame(parsing: Data([1])) }
        #expect(throws: TerminalFrame.ParseError.tooShort) { try TerminalFrame(parsing: Data([1, 5]) + Data("ab".utf8)) }
        #expect(throws: TerminalFrame.ParseError.unknownKind(3)) { try TerminalFrame(parsing: Data([3, 1, 0x61])) }
        #expect(throws: TerminalFrame.ParseError.badId) { try TerminalFrame(parsing: Data([1, 0]) + Data("x".utf8)) }
        #expect(throws: TerminalFrame.ParseError.badId) { try TerminalFrame(parsing: Data([1, 2, 0x61, 0xff])) }
    }
}
