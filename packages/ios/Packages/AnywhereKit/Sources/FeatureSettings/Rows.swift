import DesignSystem
import SwiftUI

/// A 44 pt line of an inset group: the title, then what goes at its end. The hairline above it starts at the title.
@MainActor
struct LabeledRow<Accessory: View>: View {
    let title: String
    var first = false
    @ViewBuilder let accessory: Accessory

    var body: some View {
        HStack(spacing: Space.m) {
            Text(title).lineLimit(1)
            Spacer(minLength: 0)
            accessory
        }
        .font(.system(size: 17))
        .foregroundStyle(Palette.text.color)
        .padding(.horizontal, Inset.margin)
        .frame(minHeight: HitTarget.minimum)
        .rowDivider(!first, leading: Inset.margin)
    }
}

extension LabeledRow where Accessory == Text {
    init(_ title: String, value: String, first: Bool = false) {
        self.init(title: title, first: first) { Text(value).foregroundStyle(Palette.text2.color) }
    }
}

/// A line with a switch at its end.
@MainActor
struct ToggleRow: View {
    let title: String
    @Binding var isOn: Bool
    var first = false

    init(_ title: String, isOn: Binding<Bool>, first: Bool = false) {
        self.title = title
        _isOn = isOn
        self.first = first
    }

    var body: some View {
        Toggle(title, isOn: $isOn)
            .toggleStyle(InkToggleStyle())
            .font(.system(size: 17))
            .foregroundStyle(Palette.text.color)
            .padding(.horizontal, Inset.margin)
            .frame(minHeight: HitTarget.minimum)
            .rowDivider(!first, leading: Inset.margin)
    }
}

/// A line that opens another screen: an icon, the title, the value it holds now, and a chevron.
@MainActor
struct NavRow: View {
    let icon: Lucide
    let title: String
    var value: String?
    var first = false

    var body: some View {
        HStack(spacing: 0) {
            LucideIcon(icon, size: 17).foregroundStyle(Palette.text2.color).frame(width: Inset.textColumn)
            Text(title).lineLimit(1)
            Spacer(minLength: Space.m)
            if let value { Text(value).foregroundStyle(Palette.text2.color).lineLimit(1).padding(.trailing, Space.xs) }
            LucideIcon(.chevronRight, size: 17).foregroundStyle(Palette.text4.color).padding(.trailing, Space.m)
        }
        .font(.system(size: 17))
        .foregroundStyle(Palette.text.color)
        .frame(minHeight: HitTarget.minimum)
        .rowDivider(!first)
        .contentShape(Rectangle())
    }
}

/// The small print under a group.
@MainActor
struct GroupFooter: View {
    let text: String
    var failed = false

    init(_ text: String, failed: Bool = false) {
        self.text = text
        self.failed = failed
    }

    var body: some View {
        Text(text)
            .font(.system(size: 13))
            .foregroundStyle((failed ? Palette.failedText : Palette.text3).color)
            .padding(.horizontal, Inset.margin * 2)
            .padding(.top, Space.s)
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
    }
}

/// The design's switch: an ink track with an ink-on knob when on, a quiet one with a white knob when off.
struct InkToggleStyle: ToggleStyle {
    func makeBody(configuration: Configuration) -> some View {
        Button { configuration.isOn.toggle() } label: {
            HStack(spacing: Space.m) {
                configuration.label
                Spacer(minLength: 0)
                Capsule()
                    .fill((configuration.isOn ? Palette.text : Palette.fill4).color)
                    .frame(width: 52, height: 32)
                    .overlay(alignment: configuration.isOn ? .trailing : .leading) {
                        Circle()
                            .fill(configuration.isOn ? Palette.onText.color : .white)
                            .frame(width: 28, height: 28)
                            .shadow(color: .black.opacity(0.25), radius: 2, y: 2)
                            .padding(2)
                    }
                    .animation(.snappy(duration: 0.2), value: configuration.isOn)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(.isToggle)
        .accessibilityValue(configuration.isOn ? "On" : "Off")
    }
}
