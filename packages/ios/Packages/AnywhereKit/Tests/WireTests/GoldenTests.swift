import Foundation
import Testing
@testable import Wire

/// pocketd's golden frames: the contract both sides decode. Read from its testdata at test time, so new ones count at once.
enum Golden {
    static let root = URL(fileURLWithPath: #filePath)
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .deletingLastPathComponent().deletingLastPathComponent().deletingLastPathComponent()
        .appendingPathComponent("pocketd/internal/proto/testdata/golden")

    static func files(_ side: String) -> [String] {
        let dir = root.appendingPathComponent(side)
        let names = (try? FileManager.default.contentsOfDirectory(atPath: dir.path)) ?? []
        return names.filter { $0.hasSuffix(".json") }.sorted()
    }

    static func data(_ side: String, _ name: String) throws -> Data {
        try Data(contentsOf: root.appendingPathComponent(side).appendingPathComponent(name))
    }

    static func json(_ data: Data) throws -> JSONValue {
        try JSONDecoder().decode(JSONValue.self, from: data)
    }
}

@Suite("Golden frames")
struct GoldenTests {
    @Test("the golden folders are there to read")
    func goldensExist() {
        #expect(Golden.files("server").count > 50)
        #expect(Golden.files("client").count > 40)
    }

    @Test("every server golden decodes to a known message and encodes back to the same JSON", arguments: Golden.files("server"))
    func serverGolden(_ name: String) throws {
        let data = try Golden.data("server", name)
        let frame = try ServerFrame.decode(data)
        if case .unknown(let type) = frame.message {
            Issue.record("\(name): unknown type \(type)")
        }
        #expect(try Golden.json(frame.encoded()) == Golden.json(data), "\(name)")
    }

    @Test("every client golden decodes and encodes back to the same JSON", arguments: Golden.files("client"))
    func clientGolden(_ name: String) throws {
        let data = try Golden.data("client", name)
        let frame = try ClientFrame.decode(data)
        #expect(try Golden.json(frame.encoded()) == Golden.json(data), "\(name)")
    }
}
