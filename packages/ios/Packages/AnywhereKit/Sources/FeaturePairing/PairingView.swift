import Client
import DesignSystem
import Domain
import Stores
import SwiftUI

/// Pairs this phone with a Mac: scan its code, paste its link or type its code.
@MainActor
public struct PairingView: View {
    let app: AppModel
    let link: URL?

    public init(app: AppModel, link: URL? = nil) {
        self.app = app
        self.link = link
    }

    public var body: some View {
        Placeholder("Pair with your Mac", systemImage: "qrcode.viewfinder", detail: "Open Anywhere on your Mac and scan its pairing code.")
    }
}
