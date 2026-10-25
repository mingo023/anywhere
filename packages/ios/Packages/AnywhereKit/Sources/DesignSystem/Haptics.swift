#if os(iOS)
    import UIKit
#endif

/// Taps the phone's haptic engine; does nothing where there is none.
@MainActor
public enum Haptics {
    public static func tap() {
        #if os(iOS)
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
        #endif
    }

    public static func success() {
        #if os(iOS)
            UINotificationFeedbackGenerator().notificationOccurred(.success)
        #endif
    }

    public static func warning() {
        #if os(iOS)
            UINotificationFeedbackGenerator().notificationOccurred(.warning)
        #endif
    }
}
