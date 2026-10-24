import Client
import Foundation
import Wire

// Drives a real pocketd through Client: says hello and prints what comes back, then stops.
// Usage: anywhere-probe <host:port> <device token> [seconds]
let args = CommandLine.arguments
guard args.count >= 3 else {
    print("usage: anywhere-probe <host:port> <device token> [seconds]")
    exit(2)
}
let seconds = args.count > 3 ? Double(args[3]) ?? 5 : 5
let connection = DaemonConnection(config: .init(host: args[1], token: args[2], clientId: "probe-\(ProcessInfo.processInfo.processIdentifier)"))
await connection.start()
let deadline = Task {
    try? await Task.sleep(for: .seconds(seconds))
    await connection.stop()
}
for await event in connection.events {
    switch event {
    case .connected(let ok): print("connected", ok.hostname, ok.caps ?? [])
    case .message(let m): print("message", m.type)
    case .terminal(let f): print("terminal", f.terminalId, f.payload.count)
    case .closed(let outcome): print("closed", outcome)
    }
}
deadline.cancel()
