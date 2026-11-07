import Client
import Wire

/// Why a request to a Mac failed, as the reader sees it. It names the Mac, as a phone may pair with several.
public enum ErrorText {
    /// The Mac before it says its name.
    public static let unnamedMac = "your Mac"

    /// Nil for a request that was cancelled, which needs no message.
    public static func describe(_ error: any Error, mac: String) -> String? {
        switch error {
        case DaemonError.server(let e): e.message
        case DaemonError.offline: "Not connected to \(mac)"
        case DaemonError.timedOut: "\(mac) didn't answer in time"
        case DaemonError.closed: "The connection to \(mac) dropped"
        case DaemonError.cancelled: nil
        case DaemonError.encoding: "\(mac) sent a reply this app can't read"
        default: "Something went wrong"
        }
    }
}
