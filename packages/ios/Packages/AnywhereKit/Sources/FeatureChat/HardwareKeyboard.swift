import Combine
import Foundation
#if os(iOS)
    import GameController
#endif

/// Whether a hardware keyboard is attached, so the footer hints at keys only when there are keys to press.
enum HardwareKeyboard {
    @MainActor static var attached: Bool {
        #if os(iOS)
            GCKeyboard.coalesced != nil
        #else
            true
        #endif
    }

    /// Fires as a keyboard connects or disconnects.
    @MainActor static var changes: AnyPublisher<Void, Never> {
        #if os(iOS)
            NotificationCenter.default.publisher(for: .GCKeyboardDidConnect)
                .merge(with: NotificationCenter.default.publisher(for: .GCKeyboardDidDisconnect))
                .map { _ in () }
                .eraseToAnyPublisher()
        #else
            Empty().eraseToAnyPublisher()
        #endif
    }
}
