import SwiftUI

private struct StaleSinceKey: EnvironmentKey {
    static let defaultValue: Int? = nil
}

extension EnvironmentValues {
    /// When the Mac the rows come from went offline, in ms, once what they show is stale; nil while it is online. Rows
    /// stop their timers; the screen fades them.
    var staleSince: Int? {
        get { self[StaleSinceKey.self] }
        set { self[StaleSinceKey.self] = newValue }
    }
}
