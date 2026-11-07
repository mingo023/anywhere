import DesignSystem
import Domain
import Stores
import SwiftUI

/// A Mac's Up next and Working sessions on their own, for a column beside the project tree (the iPad's content column).
@MainActor
public struct UpNextList: View {
    let host: HostModel
    @State private var action: RowAction?

    public init(host: HostModel) {
        self.host = host
    }

    public var body: some View {
        let built = host.homeList()
        let projectOf = Dictionary(built.sections.flatMap { s in s.trees.flatMap { $0.sessions.map { ($0, s.name) } } }, uniquingKeysWith: { a, _ in a })
        List {
            UpNextSections(host: host, mac: host.mac, projectOf: projectOf, action: $action)
        }
        #if os(iOS)
            .listStyle(.insetGrouped)
        #else
            .listStyle(.inset)
        #endif
        .overlay {
            if host.sessions.upNext.isEmpty && !host.sessions.sessions.values.contains(where: { $0.status == .working }) {
                ContentUnavailableView("Nothing needs you", systemImage: "checkmark.circle", description: Text("Sessions that need you, fail or finish show here."))
            }
        }
        .navigationTitle("Sessions")
        .stale(host)
        .rowActions(host, $action)
    }
}
