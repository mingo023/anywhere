import Domain
import Stores
import SwiftUI

/// What the Files screens know beyond the files: the Mac, which session takes a mention or a quote, how to open
/// another feature's screen, and the git store whose watch tells them an agent changed the tree (nil where nothing
/// can, as in the gallery).
struct FilesContext {
    var mac = ""
    var target = ChatTarget.none
    /// The project holding the tree, for a session started from it.
    var project: String?
    var open: ((Route) -> Void)?
    var git: GitStore?

    /// Lines can go to a session, or start one.
    var canAsk: Bool { target != .none || open != nil }
}

extension View {
    /// Has the Mac watch `tree` while this screen shows, so its listings and previews follow the agent's writes.
    func watching(_ tree: String, _ context: FilesContext) -> some View {
        onAppear { context.git?.show(tree) }
            .onDisappear { context.git?.hide(tree) }
    }
}
