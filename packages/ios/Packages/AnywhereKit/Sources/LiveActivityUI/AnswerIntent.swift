/// Answers an ask from the Lock Screen or the Dynamic Island. The app sets the handler at launch: it opens a one-shot
/// connection to the Mac and sends `permission.resolve`. The widget never runs it, since a `LiveActivityIntent`
/// performs in the app's process; the widget only needs the type to build its buttons.
public enum Answering {
    public typealias Handler = @MainActor @Sendable (_ mac: String, _ request: String, _ decision: String) async throws -> Void

    @MainActor public static var handler: Handler?

    public struct Unavailable: Error {}

    @MainActor
    public static func answer(mac: String, request: String, decision: String) async throws {
        guard let handler else { throw Unavailable() }
        try await handler(mac, request, decision)
    }
}

#if os(iOS) && canImport(AppIntents)
    import AppIntents

    /// App Intents in a Swift package are found only through a package the app and the widget each include.
    public struct LiveActivityIntents: AppIntentsPackage {}

    public struct AnswerIntent: LiveActivityIntent {
        public static let title: LocalizedStringResource = "Answer a session"
        public static let isDiscoverable = false

        @Parameter(title: "Mac") public var mac: String
        @Parameter(title: "Request") public var request: String
        /// allow or deny.
        @Parameter(title: "Decision") public var decision: String

        public init() {}

        public init(mac: String, request: String, decision: String) {
            self.mac = mac
            self.request = request
            self.decision = decision
        }

        public func perform() async throws -> some IntentResult {
            try await Answering.answer(mac: mac, request: request, decision: decision)
            return .result()
        }
    }
#endif
