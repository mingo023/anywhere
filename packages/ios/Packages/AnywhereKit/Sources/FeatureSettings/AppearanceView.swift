import DesignSystem
import SwiftUI

/// System, Light or Dark, as a list to pick from.
@MainActor
struct AppearanceView: View {
    @AppStorage(Appearance.key) private var appearance = Appearance.system

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                ScreenTitle("Appearance")
                VStack(spacing: 0) {
                    ForEach(Array(Appearance.allCases.enumerated()), id: \.element) { index, option in
                        Button {
                            Haptics.tap()
                            appearance = option
                        } label: {
                            LabeledRow(title: option.label, first: index == 0) {
                                if option == appearance { LucideIcon(.check, size: 17) }
                            }
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(option == appearance ? .isSelected : [])
                    }
                }
                .insetGroup()
                .padding(.top, Space.l)
                GroupFooter("System follows your iPhone's setting.")
            }
        }
        .screenBackground()
        .pushedScreen("Appearance")
    }
}
