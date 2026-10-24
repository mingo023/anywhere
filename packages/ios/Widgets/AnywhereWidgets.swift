import AppIntents
import LiveActivityUI
import SwiftUI
import WidgetKit

@main
struct AnywhereWidgets: WidgetBundle {
    var body: some Widget {
        MacActivityWidget()
    }
}

struct AnywhereWidgetIntents: AppIntentsPackage {
    static var includedPackages: [any AppIntentsPackage.Type] { [LiveActivityIntents.self] }
}
