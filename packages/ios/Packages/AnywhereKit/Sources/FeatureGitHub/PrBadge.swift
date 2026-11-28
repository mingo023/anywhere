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

    /// A state icon's colour: the saturated tone, not the one tuned for words.
    nonisolated var icon: Token {
        switch self {
        case .success: Palette.success
        case .failed: Palette.failed
        case .waiting: Palette.waiting
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
    var icon: Lucide?
    var dot = false

    var body: some View {
        HStack(spacing: Space.xs) {
            if dot { Circle().frame(width: 6, height: 6) }
            if let icon { LucideIcon(icon, size: 11) }
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
    var size: CGFloat = 16
    var tone: PrTone?

    var body: some View {
        let g = PullRequestText.glyph(state: state, draft: draft)
        LucideIcon(g.icon, size: size)
            .foregroundStyle((tone ?? g.tone).icon.color)
            .accessibilityElement()
            .accessibilityLabel(state == "OPEN" ? (draft ? "Draft" : "Open") : state.capitalized)
    }
}

/// The PR's state as the header's pill: its icon and word in the state's colour on a tint of it.
@MainActor
struct PrStateBadge: View {
    let state: String
    let draft: Bool

    var body: some View {
        let g = PullRequestText.glyph(state: state, draft: draft)
        HStack(spacing: Space.xs) {
            LucideIcon(g.icon, size: 14)
            Text(PullRequestText.stateLabel(state: state, draft: draft))
        }
        .font(.system(size: 13, weight: .semibold))
        .foregroundStyle(ink.color)
        .padding(.leading, 7)
        .padding(.trailing, 10)
        .frame(height: 26)
        .background(fill.color, in: Capsule())
        .fixedSize()
    }

    private var ink: Token {
        switch state {
        case "MERGED": Token(0x8250dfff, 0xb07fe0ff)
        case "CLOSED": Palette.failedText
        default: draft ? Palette.text2 : Palette.successText
        }
    }

    private var fill: Token {
        switch state {
        case "MERGED": Token(0x8250df1f, 0x8e4ec633)
        case "CLOSED": Palette.failedBackground
        default: draft ? Palette.fill3 : Token.fixed(0x30a46c29)
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
                LucideIcon(.chevronLeft, size: 11).foregroundStyle(Palette.text3.color)
                BranchChip(name: head)
            }
            if let commits { Text("· \(PullRequestText.commits(commits))").foregroundStyle(Palette.text3.color).lineLimit(1).fixedSize() }
        }
        .font(Typography.footnote)
        .accessibilityElement(children: .ignore)
        .accessibilityLabel((head.map { "\($0) into \(PullRequestText.baseBranch(base))" } ?? "Into \(PullRequestText.baseBranch(base))") + (commits.map { ", \(PullRequestText.commits($0))" } ?? ""))
    }
}
