import Wire

/// How a connection to a Mac ended.
public enum CloseOutcome: Hashable, Sendable {
    /// It dropped; the connection dials again after `seconds`.
    case retrying(seconds: Double)
    /// The Mac no longer knows this phone: pair again.
    case revoked
    /// The Mac's pocketd is older than this phone speaks.
    case updateMac
    /// This phone is older than the Mac speaks.
    case updatePhone
    /// The app closed it.
    case stopped

    /// The end a close code and the last error code before it stand for, or nil when the phone should just redial.
    public static func final(closeCode: Int?, errorCode: ErrorCode?) -> CloseOutcome? {
        if closeCode == CloseCode.unpaired || errorCode == .notPaired || errorCode == .revoked { return .revoked }
        if closeCode == CloseCode.versionMismatch || errorCode == .clientTooOld || errorCode == .serverTooOld {
            return errorCode == .serverTooOld ? .updateMac : .updatePhone
        }
        return nil
    }
}
