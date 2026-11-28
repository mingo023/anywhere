import ActivityModel
import DesignSystem
import SwiftUI

/// The expanded Dynamic Island's content as the system lays it out: the Mac and the clock flanking the sensor, the session under them.
@MainActor
struct ExpandedIsland: View {
    let snapshot: ActivitySnapshot

    var body: some View {
        VStack(spacing: Space.m) {
            HStack(alignment: .center) {
                IslandRegion.expandedLeading(snapshot)
                Spacer(minLength: Space.s)
                IslandRegion.expandedTrailing(snapshot)
            }
            IslandRegion.expandedBottom(snapshot)
        }
        .environment(\.colorScheme, .dark)
    }
}

/// The diagonal stripes the design puts behind what the system draws: a wallpaper, or "any app".
@MainActor
struct StripedWallpaper: View {
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let dark = scheme == .dark
        let base = Color(white: dark ? 0.078 : 0.93)
        let stripe = Color(white: dark ? 0.094 : 0.90)
        Canvas { canvas, size in
            canvas.fill(Path(CGRect(origin: .zero, size: size)), with: .color(base))
            canvas.rotate(by: .degrees(45))
            let reach = size.width + size.height
            var x: CGFloat = 14
            while x < reach {
                canvas.fill(Path(CGRect(x: x, y: -reach, width: 14, height: reach * 2)), with: .color(stripe))
                x += 28
            }
        }
        .ignoresSafeArea()
    }
}

/// A line of mono that labels the stand-in for what the system draws.
private struct StandIn: View {
    let text: String

    var body: some View {
        Text(text).font(.system(size: 11, design: .monospaced)).foregroundStyle(Palette.text4.color)
    }
}

/// The island's black, edged by a hairline so it reads on a dark screen.
private struct Island<S: InsettableShape>: View {
    let shape: S

    var body: some View {
        shape.fill(Color(white: 0.165)).overlay(shape.inset(by: 1).fill(.black))
    }
}

/// Frame 41: the expanded Dynamic Island over any app.
@MainActor
struct ExpandedFrame: View {
    let snapshot: ActivitySnapshot

    var body: some View {
        ZStack(alignment: .top) {
            StripedWallpaper()
            StandIn(text: "any app").frame(maxHeight: .infinity).offset(y: 44)
            ExpandedIsland(snapshot: snapshot)
                .padding(.horizontal, 22).padding(.vertical, 20)
                .background(.black, in: RoundedRectangle(cornerRadius: 44, style: .continuous))
                .padding(.horizontal, 10)
                .padding(.top, 11)
        }
    }
}

/// Frame 41b: the compact and minimal islands over the Lock Screen's banner.
@MainActor
struct VariantsFrame: View {
    let snapshot: ActivitySnapshot
    @Environment(\.colorScheme) private var scheme

    var body: some View {
        let dark = scheme == .dark
        VStack(alignment: .leading, spacing: 28) {
            section("Compact") {
                HStack {
                    IslandRegion.compactLeading(snapshot)
                    Spacer()
                    IslandRegion.compactTrailing(snapshot)
                }
                .padding(.horizontal, 14)
                .frame(width: 250, height: 37)
                .background(Island(shape: Capsule()))
                .frame(maxWidth: .infinity)
            }
            section("Minimal") {
                HStack(spacing: Space.s) {
                    Island(shape: Capsule()).frame(width: 126, height: 37)
                    IslandRegion.minimal(snapshot)
                        .frame(width: 37, height: 37)
                        .background(Island(shape: Circle()))
                }
                .frame(maxWidth: .infinity)
            }
            section("Lock Screen") {
                LockScreenActivity(snapshot)
                    .background(Color.of(.fixed(dark ? 0x2c2c30e6 : 0xffffffe6), dark: dark), in: RoundedRectangle(cornerRadius: 22, style: .continuous))
            }
        }
        .padding(.horizontal, Space.m)
        .padding(.vertical, 28)
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
        .background(Palette.background.color.ignoresSafeArea())
    }

    private func section(_ title: String, @ViewBuilder content: () -> some View) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title).font(.system(size: 13, weight: .semibold)).foregroundStyle(Palette.text2.color).padding(.horizontal, Space.m)
            content()
        }
    }
}

/// Frame 40: a needs-you notification on the Lock Screen. The words are the design's: the banner itself is the system's.
@MainActor
struct PushNotificationFrame: View {
    @Environment(\.colorScheme) private var scheme

    static let title = "Needs you"
    static let line = "pnpm test checkout --repeat 20"
    static let place = "anywhere · flaky-checkout · MacBook Pro"

    var body: some View {
        let dark = scheme == .dark
        ZStack(alignment: .top) {
            StripedWallpaper()
            StandIn(text: "lock screen wallpaper").frame(maxHeight: .infinity)
            LucideIcon(.lock, size: 17).foregroundStyle(Palette.text.color).padding(.top, 58)
            Text("Tuesday 24 November")
                .font(.system(size: 20, weight: .semibold)).foregroundStyle(Palette.text.color.opacity(0.85)).padding(.top, 96)
            Text("9:41")
                .font(.system(size: 104, weight: .semibold)).tracking(-3).monospacedDigit().foregroundStyle(Palette.text.color).padding(.top, 122)
            VStack(spacing: 0) {
                banner(dark: dark)
                Color.of(.fixed(dark ? 0x2c2c3080 : 0xffffff80), dark: dark)
                    .frame(height: 10)
                    .clipShape(UnevenRoundedRectangle(bottomLeadingRadius: 16, bottomTrailingRadius: 16, style: .continuous))
                    .padding(.horizontal, Space.m)
            }
            .padding(.horizontal, Space.s)
            .padding(.top, 560)
            HStack {
                shortcut(.flashlight)
                Spacer()
                shortcut(.camera)
            }
            .padding(.horizontal, 46)
            .padding(.bottom, 46)
            .frame(maxHeight: .infinity, alignment: .bottom)
        }
    }

    private func banner(dark: Bool) -> some View {
        HStack(alignment: .top, spacing: Space.m) {
            LucideIcon(.monitorSmartphone, size: 22)
                .foregroundStyle(Palette.onText.color)
                .frame(width: 38, height: 38)
                .background(Palette.text.color, in: RoundedRectangle(cornerRadius: 9, style: .continuous))
            VStack(alignment: .leading, spacing: 2) {
                HStack(alignment: .firstTextBaseline) {
                    Text(Self.title).font(.system(size: 15, weight: .semibold))
                    Spacer()
                    Text("now").font(.system(size: 13)).foregroundStyle(Palette.text2.color)
                }
                Text(Self.line).font(.system(size: 14, design: .monospaced)).lineLimit(1)
                Text(Self.place).font(.system(size: 13)).foregroundStyle(Palette.text2.color).lineLimit(1)
            }
            .foregroundStyle(Palette.text.color)
        }
        .padding(14)
        .background(Color.of(.fixed(dark ? 0x2c2c30b8 : 0xffffffb8), dark: dark), in: RoundedRectangle(cornerRadius: 22, style: .continuous))
    }

    private func shortcut(_ icon: Lucide) -> some View {
        LucideIcon(icon, size: 22)
            .foregroundStyle(Palette.text.color)
            .frame(width: 50, height: 50)
            .background(Palette.fill3.color, in: Circle())
    }
}
