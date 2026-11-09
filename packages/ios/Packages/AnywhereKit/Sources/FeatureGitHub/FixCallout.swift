import ChatModel
import DesignSystem
import SwiftUI

/// What there is to fix on the PR, with Send all to its agent; a phone that may only watch is told how to get it.
@MainActor
struct FixCallout: View {
    let checks: Int
    let comments: Int
    /// The button's title, nil when nothing can be sent.
    let send: String?
    let canDrive: Bool
    let mac: PrMac
    let action: () -> Void

    var body: some View {
        let (title, detail) = PullRequestText.fixSummary(checks: checks, comments: comments)
        Section {
            Label {
                VStack(alignment: .leading, spacing: Space.xxs) {
                    Text(title).font(Typography.headline).foregroundStyle(Palette.text.color)
                    Text(detail).font(Typography.footnote).foregroundStyle(Palette.text3.color)
                }
            } icon: {
                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(PrTone.failed.text.color)
            }
            if let send {
                Button {
                    Haptics.tap()
                    action()
                } label: {
                    Text(send).frame(maxWidth: .infinity)
                }
                .buttonStyle(CapsuleButtonStyle())
                .disabled(mac.offline)
            }
        } footer: {
            if !canDrive { Text(Composer.viewOnly(mac: mac.name)) }
        }
    }
}
