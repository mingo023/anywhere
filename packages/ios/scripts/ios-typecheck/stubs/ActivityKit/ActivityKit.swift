// Signatures copied from developer.apple.com/documentation/activitykit (declarations and conformances as listed there).
// Bodies are fatalError: this module exists only to typecheck callers.
import Foundation

public protocol ActivityAttributes: Decodable, Encodable {
    associatedtype ContentState: Decodable, Encodable, Hashable
}

public enum ActivityState: Decodable, Encodable, Equatable, Hashable, Sendable {
    case active, ended, dismissed, stale, pending
}

public struct ActivityUIDismissalPolicy: Equatable, Sendable {
    public static var `default`: ActivityUIDismissalPolicy { fatalError() }
    public static var immediate: ActivityUIDismissalPolicy { fatalError() }
    public static func after(_ date: Date) -> ActivityUIDismissalPolicy { fatalError() }
}

public struct PushType: Equatable {
    public static var token: PushType { fatalError() }
}

public struct ActivityContent<State>: CustomStringConvertible, Sendable where State: Decodable, State: Encodable, State: Hashable {
    public init(state: State, staleDate: Date?, relevanceScore: Double = 0.0) { fatalError() }
    public var state: State { fatalError() }
    public var description: String { "" }
}

public struct StubSequence<Element>: AsyncSequence {
    public struct AsyncIterator: AsyncIteratorProtocol {
        public mutating func next() async -> Element? { nil }
    }
    public func makeAsyncIterator() -> AsyncIterator { AsyncIterator() }
}

public final class ActivityAuthorizationInfo {
    public init() {}
    public final var areActivitiesEnabled: Bool { false }
    public final var frequentPushesEnabled: Bool { false }
    public struct FrequentPushEnablementUpdates: AsyncSequence {
        public typealias Element = Bool
        public struct AsyncIterator: AsyncIteratorProtocol { public mutating func next() async -> Bool? { nil } }
        public func makeAsyncIterator() -> AsyncIterator { AsyncIterator() }
    }
    public final let frequentPushEnablementUpdates = FrequentPushEnablementUpdates()
}

public class Activity<Attributes>: Identifiable where Attributes: ActivityAttributes {
    public typealias ContentState = Attributes.ContentState
    public final let id: String = ""
    public final let attributes: Attributes
    init(_ a: Attributes) { attributes = a }
    public var activityState: ActivityState { fatalError() }
    public var content: ActivityContent<ContentState> { fatalError() }
    public var pushToken: Data? { nil }

    public static var activities: [Activity<Attributes>] { [] }

    public struct PushTokenUpdates: AsyncSequence {
        public typealias Element = Data
        public struct AsyncIterator: AsyncIteratorProtocol { public mutating func next() async -> Data? { nil } }
        public func makeAsyncIterator() -> AsyncIterator { AsyncIterator() }
    }
    public struct ActivityStateUpdates: AsyncSequence {
        public typealias Element = ActivityState
        public struct AsyncIterator: AsyncIteratorProtocol { public mutating func next() async -> ActivityState? { nil } }
        public func makeAsyncIterator() -> AsyncIterator { AsyncIterator() }
    }
    public struct ActivityUpdates: AsyncSequence {
        public typealias Element = Activity<Attributes>
        public struct AsyncIterator: AsyncIteratorProtocol { public mutating func next() async -> Activity<Attributes>? { nil } }
        public func makeAsyncIterator() -> AsyncIterator { AsyncIterator() }
    }

    public var pushTokenUpdates: PushTokenUpdates { PushTokenUpdates() }
    public var activityStateUpdates: ActivityStateUpdates { ActivityStateUpdates() }
    public static var activityUpdates: ActivityUpdates { ActivityUpdates() }
    public static var pushToStartTokenUpdates: PushTokenUpdates { PushTokenUpdates() }

    public static func request(attributes: Attributes, content: ActivityContent<ContentState>, pushType: PushType? = nil) throws -> Activity<Attributes> { fatalError() }
    public func update(_ content: ActivityContent<ContentState>) async {}
    public func end(_ content: ActivityContent<ContentState>?, dismissalPolicy: ActivityUIDismissalPolicy = .default) async {}
}
