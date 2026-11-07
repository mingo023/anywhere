import Domain
import SwiftUI

/// A strip across a screen whose Mac is offline: what it shows is as of `asOf` (ms), or, with no time, the link is
/// reconnecting, as above the chat composer. The screen keeps its content readable and turns off its remote writes.
@MainActor
public struct StaleStrip: View {
    let mac: String
    let asOf: Int?

    public init(mac: String, asOf: Int? = nil) {
        self.mac = mac
        self.asOf = asOf
    }

    public var body: some View {
        HStack(spacing: Space.s) {
            Image(systemName: "wifi.slash").imageScale(.small)
            Text(Self.text(mac: mac, asOf: asOf)).lineLimit(1)
            Spacer(minLength: 0)
        }
        .font(Typography.footnote.weight(.medium))
        .foregroundStyle(Palette.waitingText.color)
        .padding(.horizontal, Space.l)
        .padding(.vertical, Space.s)
        .frame(maxWidth: .infinity)
        .background(Palette.waitingBackground.color)
        .accessibilityElement(children: .combine)
    }

    nonisolated static func text(mac: String, asOf: Int?, timeZone: TimeZone = .current) -> String {
        guard let asOf else { return "\(mac) is offline · reconnecting…" }
        return "\(mac) is offline · as of \(TimeText.clock(ms: asOf, timeZone: timeZone))"
    }
}
