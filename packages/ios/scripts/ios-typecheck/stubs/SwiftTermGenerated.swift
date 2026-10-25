// What SwiftTerm's build plugin and SwiftPM's resource accessor generate, enough to typecheck it.
import Foundation

public enum SwiftTermBuildInfo {
    public static let branch: String? = nil
    public static let tag: String? = nil
    public static let commit: String? = nil
    public static let hasUncommittedChanges: Bool? = nil
    public static let version = "unknown"
}

extension Foundation.Bundle {
    static let module = Bundle.main
}
