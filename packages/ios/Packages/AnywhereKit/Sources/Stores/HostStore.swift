import Wire

/// A store fed by one Mac's connection. `HostModel` hands every message to every store; each takes what it owns.
@MainActor
public protocol HostStore: AnyObject {
    /// Takes in a message; true when it changed anything this store owns.
    func apply(_ message: ServerMessage) -> Bool
    /// hello.ok came; the snapshot follows. Resubscribe here.
    func connected(_ hello: HelloOk)
    /// The connection dropped or ended.
    func disconnected()
}

extension HostStore {
    public func connected(_ hello: HelloOk) {}
    public func disconnected() {}
}
