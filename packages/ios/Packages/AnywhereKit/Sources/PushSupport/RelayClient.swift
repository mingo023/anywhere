import Foundation
import Wire

/// What a relay handle pushes as.
public enum RelayKind: String, Codable, Sendable {
    /// Alerts the Notification Service Extension opens.
    case alert
    /// Updates and ends of one Live Activity.
    case activity
    /// Push-to-start of a Live Activity.
    case start
}

/// The APNs environment a token was minted in: development for Xcode builds, production for TestFlight and the App Store.
public enum PushEnv: String, Codable, Sendable {
    case development
    case production
}

extension Data {
    /// An APNs token as the relay takes it.
    public var hex: String { map { String(format: "%02x", $0) }.joined() }
}

/// The relay's `/v1/register`: seals a token into a handle only the relay can open.
public struct RelayClient: Sendable {
    public static let defaultBase = URL(string: "https://push.getanywhere.dev")!

    public enum Failure: Error, Hashable, Sendable {
        case refused(status: Int, reason: String?)
        case unreadable
    }

    public let base: URL
    let session: URLSession

    public init(base: URL = RelayClient.defaultBase, session: URLSession = .shared) {
        self.base = base
        self.session = session
    }

    public func register(token: String, kind: RelayKind, env: PushEnv) async throws -> PushHandle {
        var request = URLRequest(url: base.appendingPathComponent("v1/register"))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(["token": token, "kind": kind.rawValue, "env": env.rawValue])
        request.timeoutInterval = 15
        let (data, response) = try await session.data(for: request)
        let status = (response as? HTTPURLResponse)?.statusCode ?? 0
        guard status == 200 else {
            let reason = (try? JSONDecoder().decode([String: String].self, from: data))?["reason"]
            throw Failure.refused(status: status, reason: reason)
        }
        guard let handle = try? JSONDecoder().decode(PushHandle.self, from: data) else { throw Failure.unreadable }
        return handle
    }
}

/// Handles already sealed for a token, so registering with several Macs asks the relay once.
public struct HandleCache: Sendable {
    struct Key: Hashable, Sendable {
        let token: String
        let kind: RelayKind
        let env: PushEnv
    }

    /// A handle this close to expiring is sealed anew.
    public static let renewWithin: TimeInterval = 7 * 24 * 3600

    var handles: [Key: PushHandle] = [:]

    public init() {}

    public func handle(token: String, kind: RelayKind, env: PushEnv, now: Date) -> PushHandle? {
        guard let h = handles[Key(token: token, kind: kind, env: env)], Double(h.expiresAt) - now.timeIntervalSince1970 > Self.renewWithin else { return nil }
        return h
    }

    public mutating func keep(_ handle: PushHandle, token: String, kind: RelayKind, env: PushEnv) {
        handles[Key(token: token, kind: kind, env: env)] = handle
    }
}
