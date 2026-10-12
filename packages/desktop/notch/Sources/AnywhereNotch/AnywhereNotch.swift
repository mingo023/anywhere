import AppKit
import SwiftUI

/// Reads the desktop's pending asks from stdin, a JSON line `{"asks": [...]}` each time they change,
/// and writes each answer to stdout as `{"answer": "allow" | "deny" | "reply" | "questions" | "open", "agent", "request", "text", "answers"}`.
@main
enum AnywhereNotch {
    static func main() {
        let app = NSApplication.shared
        app.setActivationPolicy(.accessory)
        let controller = Controller()
        Thread {
            while let line = readLine() {
                guard let snapshot = try? JSONDecoder().decode(Snapshot.self, from: Data(line.utf8)) else { continue }
                DispatchQueue.main.async { controller.show(snapshot.asks) }
            }
            // The desktop quit.
            exit(0)
        }.start()
        app.run()
    }
}

struct Snapshot: Decodable {
    var asks: [Ask]
}
