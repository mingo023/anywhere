import Domain
import Observation

/// The navigation stack, as `Route` values, so deep links and restoration can set it.
@MainActor
@Observable
public final class Navigator {
    public var path: [Route] = []
    /// A link that arrived before its Mac connected, opened once it does.
    public private(set) var held: DeepLink?

    public init() {}

    public func open(_ link: DeepLink, macKnown: Bool) {
        if case .pair = link {
            path = link.routes
        } else if macKnown {
            held = nil
            path = Array(link.routes.dropFirst())
        } else {
            held = link
        }
    }

    /// Opens the held link once its Mac is known.
    public func release(knownMacs: Set<String>) {
        guard let link = held else { return }
        if case .agent(let mac, _) = link, knownMacs.contains(mac) { open(link, macKnown: true) }
        if case .mac(let mac) = link, knownMacs.contains(mac) { open(link, macKnown: true) }
    }
}
