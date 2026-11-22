import DesignSystem
import Stores
import SwiftUI
import Wire

extension PrTone {
    nonisolated var text: Token {
        switch self {
        case .success: Palette.successText
        case .failed: Palette.failedText
        case .waiting: Palette.waitingText
        case .merged: Palette.merged
        case .muted: Palette.text4
        case .quiet: Palette.text3
        }
    }

    /// A pill's fill behind `text`; merged takes GitHub's purple at 12%.
    nonisolated var fill: Token {
        switch self {
        case .success: Palette.successBackground
        case .failed: Palette.failedBackground
        case .waiting: Palette.waitingBackground
        case .merged: Token.fixed(0x8250df1f)
        case .muted, .quiet: Palette.fill3
        }
    }
}

/// A capsule in a PR tone: the verdict, a gate, a thread's state.
@MainActor
struct PrPill: View {
    let text: String
    let tone: PrTone
    var systemImage: String?
    var dot = false

    var body: some View {
        HStack(spacing: Space.xs) {
            if dot { Circle().frame(width: 6, height: 6) }
            if let systemImage { Image(systemName: systemImage).imageScale(.small) }
            Text(text).lineLimit(1)
        }
        .font(Typography.captionStrong)
        .foregroundStyle(tone.text.color)
        .padding(.horizontal, Space.s)
        .padding(.vertical, Space.xxs + 1)
        .background(tone.fill.color, in: Capsule())
    }
}

/// The PR's state icon, as GitHub draws it.
@MainActor
struct PrGlyph: View {
    let state: String
    let draft: Bool

    var body: some View {
        let g = PullRequestText.glyph(state: state, draft: draft)
        Image(systemName: g.systemImage)
            .font(.system(.body, weight: .semibold))
            .foregroundStyle(g.tone.text.color)
            .frame(width: 22)
            .accessibilityLabel(state == "OPEN" ? (draft ? "Draft" : "Open") : state.capitalized)
    }
}

/// The PR's state as GitHub's header badge: its icon and word in white on the state's colour.
@MainActor
struct PrStateBadge: View {
    let state: String
    let draft: Bool

    var body: some View {
        let g = PullRequestText.glyph(state: state, draft: draft)
        HStack(spacing: Space.xs) {
            Image(systemName: g.systemImage).font(.system(size: 12, weight: .semibold))
            Text(PullRequestText.stateLabel(state: state, draft: draft))
        }
        .font(Typography.subheadline.weight(.semibold))
        .foregroundStyle(.white)
        .padding(.horizontal, Space.m)
        .padding(.vertical, Space.xs + 1)
        .background(fill.color, in: Capsule())
        .fixedSize()
    }

    /// Primer's `fgColor.open`, `done`, `closed` and `neutral` emphasis fills.
    private var fill: Token {
        switch state {
        case "MERGED": Token(0x8250dfff, 0x8957e5ff)
        case "CLOSED": Token(0xcf222eff, 0xda3633ff)
        default: draft ? Token(0x59636eff, 0x656c76ff) : Token(0x1f883dff, 0x238636ff)
        }
    }
}

/// A branch name as GitHub sets it: monospaced on a pale blue.
@MainActor
struct BranchChip: View {
    let name: String

    var body: some View {
        Text(name)
            .font(Typography.codeSmall)
            .foregroundStyle(Token(0x0969daff, 0x4493f8ff).color)
            .lineLimit(1)
            .truncationMode(.middle)
            .padding(.horizontal, Space.xs + 2)
            .padding(.vertical, Space.xxs)
            .background(Token(0xddf4ffff, 0x388bfd26).color, in: RoundedRectangle(cornerRadius: 6, style: .continuous))
    }
}

/// GitHub's compare line: the base the branch goes into, an arrow from the branch, and how many commits.
@MainActor
struct BranchRoute: View {
    let base: String
    let head: String?
    var commits: Int?

    var body: some View {
        HStack(spacing: Space.xs) {
            if head == nil { Text("into").foregroundStyle(Palette.text3.color) }
            BranchChip(name: PullRequestText.baseBranch(base)).layoutPriority(1)
            if let head {
                Image(systemName: "arrow.left").font(.system(size: 11, weight: .semibold)).foregroundStyle(Palette.text3.color)
                BranchChip(name: head)
            }
            if let commits { Text("· \(PullRequestText.commits(commits))").foregroundStyle(Palette.text3.color).lineLimit(1).fixedSize() }
        }
        .font(Typography.footnote)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel(head.map { "\($0) into \(PullRequestText.baseBranch(base))" } ?? "Into \(PullRequestText.baseBranch(base))")
    }
}
