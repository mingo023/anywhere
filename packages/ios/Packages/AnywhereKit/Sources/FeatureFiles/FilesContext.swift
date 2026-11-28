import Domain
import Stores
import SwiftUI

/// What the Files screens know beyond the files: the Mac, which session takes a mention or a quote, how to open
/// another feature's screen, and the git store whose watch tells them an agent changed the tree (nil where nothing
/// can, as in the gallery).
struct FilesContext {
    var mac = ""
    /// As read when the screen opened, for what the screens offer.
    var target = ChatTarget.none
    /// Reads the target again when something is handed, so the screens needn't watch every session; nil keeps `target`.
    var retarget: (@MainActor () -> ChatTarget)?
    /// The project holding the tree, for a session started from it.
    var project: String?
    var open: ((Route) -> Void)?
    var git: GitStore?
    /// A chat opened the screen, so a file pins Mention in Chat.
    var fromChat = false
    /// Set while the Mac can't be reached.
    var offline: Offline?

    /// Lines can go to a session, or start one.
    var canAsk: Bool { target != .none || open != nil }

    /// The Mac that went, and when, in Unix milliseconds.
    struct Offline: Equatable {
        let mac: String
        let since: Int?

        /// What the tree says when it can't be read: the Mac is gone, and nothing of it is kept here.
        func note(timeZone: TimeZone = .current) -> String {
            let lastSeen = since.map { " · last seen \(TimeText.clock(ms: $0, timeZone: timeZone))" } ?? ""
            return "\(mac) is offline\(lastSeen). Files aren't saved on this iPhone."
        }
    }
}

extension View {
    /// Has the Mac watch `tree` while this screen shows, so its listings and previews follow the agent's writes.
    func watching(_ tree: String, _ context: FilesContext) -> some View {
        onAppear { context.git?.show(tree) }
            .onDisappear { context.git?.hide(tree) }
    }
}
