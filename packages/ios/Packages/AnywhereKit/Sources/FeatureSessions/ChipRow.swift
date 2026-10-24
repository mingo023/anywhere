import DesignSystem
import SwiftUI

/// A row of choices as capsules, the picked one filled: models, efforts, access.
@MainActor
struct ChipRow<Value: Hashable>: View {
    let options: [(value: Value, label: String)]
    @Binding var selection: Value

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: Space.s) {
                ForEach(options, id: \.value) { option in
                    let picked = option.value == selection
                    Button {
                        Haptics.tap()
                        selection = option.value
                    } label: {
                        Text(option.label)
                            .font(Typography.subheadline.weight(.medium))
                            .padding(.horizontal, Space.m)
                            .padding(.vertical, Space.xs + 2)
                            .foregroundStyle(picked ? Palette.onText.color : Palette.text.color)
                            .background(picked ? Palette.text.color : Palette.fill3.color, in: Capsule())
                    }
                    .buttonStyle(.plain)
                    .accessibilityAddTraits(picked ? .isSelected : [])
                }
            }
            .padding(.vertical, Space.xxs)
        }
    }
}
