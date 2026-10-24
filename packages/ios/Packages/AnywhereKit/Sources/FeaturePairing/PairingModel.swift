import Client
import DesignSystem
import Foundation
import Observation
import Stores
#if os(iOS)
    import UIKit
#endif

/// A screen of the pairing flow after the welcome.
public enum PairingStep: Hashable, Sendable {
    case scan
    case enter
    case confirm(PairingLink)
}

/// The pairing flow: where it is, and the one pairing attempt in flight.
@MainActor
@Observable
public final class PairingModel {
    public enum Phase: Hashable, Sendable {
        case idle
        case pairing
        case failed(PairingFailure)
        case paired(String)
    }

    public var path: [PairingStep] = []
    public internal(set) var phase = Phase.idle
    /// The address of a Mac that revoked this phone, so only a new code is needed.
    public let knownHost: String?
    @ObservationIgnored let app: AppModel
    @ObservationIgnored let phoneName: String

    public init(app: AppModel, link: URL? = nil, knownHost: String? = nil, phoneName: String? = nil) {
        self.app = app
        self.knownHost = knownHost
        self.phoneName = phoneName ?? PairingModel.deviceName
        if let link, let pair = PairingEntry.link(from: link.absoluteString) {
            path = [.confirm(pair)]
        } else if knownHost != nil {
            path = [.enter]
        }
    }

    /// Opens the confirm screen for scanned or pasted text; false when it holds no pairing code.
    @discardableResult
    public func open(_ text: String) -> Bool {
        guard let link = PairingEntry.link(from: text) else { return false }
        confirm(link)
        return true
    }

    public func confirm(_ link: PairingLink) {
        phase = .idle
        if case .confirm? = path.last { path.removeLast() }
        path.append(.confirm(link))
    }

    public func pair(_ link: PairingLink) async {
        guard phase != .pairing else { return }
        phase = .pairing
        do {
            let host = try await app.pair(link, phoneName: phoneName)
            phase = .paired(host.name)
            Haptics.success()
        } catch {
            phase = .failed(error)
            Haptics.warning()
        }
    }

    /// The name the Mac lists this phone under.
    static var deviceName: String {
        #if os(iOS)
            UIDevice.current.name
        #else
            Host.current().localizedName ?? "Mac"
        #endif
    }
}
