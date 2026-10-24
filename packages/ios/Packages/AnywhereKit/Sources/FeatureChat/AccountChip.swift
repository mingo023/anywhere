import ChatModel
import DesignSystem
import Domain
import SwiftUI
import Wire

/// A chat agent's account, when the Mac has several for its provider: its name and peak usage, opening on its usage and
/// a page to switch.
@MainActor
struct AccountChip: View {
    let accounts: [Account]
    let current: Account
    let provider: String
    let machine: String
    let switchTo: (String) -> Void
    @State private var open = false
    @State private var switching = false

    static func peak(_ a: Account) -> Double { max(a.fiveHour?.pct ?? 0, a.week?.pct ?? 0) }

    var body: some View {
        Button {
            switching = false
            open = true
        } label: {
            HStack(spacing: 5) {
                ProviderIcon(provider: provider, size: 10)
                Text(current.name).lineLimit(1)
                UsageBar(pct: Self.peak(current)).frame(width: 22)
                Text("\(Int(Self.peak(current)))%").monospacedDigit()
            }
            .font(.system(size: 11))
            .foregroundStyle(Palette.text3.color)
            .frame(minHeight: 24)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .popover(isPresented: $open) {
            ZStack {
                if switching { switchPage.transition(.offset(x: 48).combined(with: .opacity)) } else { usagePage.transition(.offset(x: -48).combined(with: .opacity)) }
            }
            .animation(.easeOut(duration: 0.24), value: switching)
            .padding(Space.m)
            .frame(width: 300)
            .presentationCompactAdaptation(.popover)
        }
    }

    private var usagePage: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Text("\(who(provider)) usage").font(ChatFont.small.weight(.semibold))
            Text(["In use: \(current.name)", current.plan, current.email].compactMap { $0 }.joined(separator: " · "))
                .font(ChatFont.caption).foregroundStyle(Palette.text3.color)
            if let five = current.fiveHour { block("5-hour", five) }
            if let week = current.week { block("Weekly", week) }
            Button {
                switching = true
            } label: {
                HStack {
                    Text("Switch account · \(accounts.count - 1) more")
                    Spacer()
                    Image(systemName: "chevron.right")
                }
                .font(ChatFont.small)
                .frame(minHeight: 32)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
    }

    private func block(_ title: String, _ usage: Wire.Usage) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title).font(ChatFont.caption.weight(.medium))
            UsageBar(pct: usage.pct)
            HStack {
                Text("\(Int(usage.pct))% used")
                Spacer()
                Text("\(100 - Int(usage.pct))% remaining")
            }
            .font(.system(size: 11)).monospacedDigit().foregroundStyle(Palette.text3.color)
            if let resets = usage.resetsAt {
                Text("Resets in \(TimeText.elapsed(seconds: max(resets - Int(Date().timeIntervalSince1970), 0)))")
                    .font(.system(size: 11)).foregroundStyle(Palette.text4.color)
            }
        }
    }

    private var switchPage: some View {
        VStack(alignment: .leading, spacing: Space.s) {
            Button {
                switching = false
            } label: {
                Label("Usage", systemImage: "chevron.left").font(ChatFont.small)
            }
            .buttonStyle(.plain)
            ForEach(accounts) { a in
                Button {
                    switchTo(a.id)
                    open = false
                } label: {
                    HStack(spacing: 8) {
                        Image(systemName: "checkmark").opacity(a.id == current.id ? 1 : 0)
                        VStack(alignment: .leading, spacing: 2) {
                            HStack(spacing: 6) {
                                Text(a.name)
                                if let plan = a.plan { Pill(plan) }
                                if !a.signedIn { Text("Signed out").foregroundStyle(Palette.text4.color) }
                            }
                            Text("5h \(Int(a.fiveHour?.pct ?? 0))% · wk \(Int(a.week?.pct ?? 0))%")
                                .font(.system(size: 11)).monospacedDigit().foregroundStyle(Palette.text3.color)
                            UsageBar(pct: Self.peak(a))
                        }
                    }
                    .font(ChatFont.small)
                    .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .disabled(!a.signedIn || a.id == current.id)
            }
            Text("Only \(who(provider)) accounts on \(machine)").font(.system(size: 11)).foregroundStyle(Palette.text4.color)
        }
    }
}

/// A thin usage bar, amber from 75%, red from 90%.
@MainActor
struct UsageBar: View {
    let pct: Double

    var body: some View {
        GeometryReader { g in
            ZStack(alignment: .leading) {
                Capsule().fill(Palette.fill4.color)
                Capsule().fill(tint.color).frame(width: g.size.width * min(max(pct, 0), 100) / 100)
            }
        }
        .frame(height: 4)
    }

    private var tint: Token {
        switch ContextMeter.level(Int(pct), 100) {
        case .low: Palette.text3
        case .warn: Palette.waiting
        case .danger: Palette.failed
        }
    }
}
