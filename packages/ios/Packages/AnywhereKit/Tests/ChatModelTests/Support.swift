import Foundation
import Testing
import Wire

@testable import ChatModel

/// Items in the order given, each numbered by its place unless it says otherwise, stamped a second apart.
func items(_ json: String) -> [TimelineItem] {
    let list = try! JSONSerialization.jsonObject(with: Data(json.utf8)) as! [[String: Any]]
    return list.enumerated().map { i, v in
        var v = v
        let n = i + 1
        if v["seq"] == nil { v["seq"] = n }
        if v["id"] == nil { v["id"] = "i\(n)" }
        v["ts"] = n * 1000
        return decode(v)
    }
}

/// One item as pocketd sends it; a result may leave out its duration.
func decode(_ v: [String: Any]) -> TimelineItem {
    var v = v
    if v["kind"] as? String == "result" && v["durationMs"] == nil { v["durationMs"] = 0 }
    let data = try! JSONSerialization.data(withJSONObject: v)
    return try! JSONDecoder().decode(TimelineItem.self, from: data)
}

/// Item `seq`, its id `i{seq}` and stamped at `seq`.
func item(_ seq: Int, _ json: String) -> TimelineItem {
    var v = try! JSONSerialization.jsonObject(with: Data(json.utf8)) as! [String: Any]
    v["seq"] = seq
    v["id"] = "i\(seq)"
    v["ts"] = seq
    return decode(v)
}

func toolJSON(_ id: String, _ name: String, _ detail: String, _ status: String) -> String {
    #"{"kind": "tool", "call": {"toolUseId": "\#(id)", "name": "\#(name)", "detail": \#(detail), "status": "\#(status)"}}"#
}

func golden(_ name: String) -> [String: Any] {
    let url = URL(fileURLWithPath: #filePath).deletingLastPathComponent()
        .appendingPathComponent("../../../../../pocketd/internal/proto/testdata/golden/server/\(name).json")
    return try! JSONSerialization.jsonObject(with: Data(contentsOf: url)) as! [String: Any]
}
