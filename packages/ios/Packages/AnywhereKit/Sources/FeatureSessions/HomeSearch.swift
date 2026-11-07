import SwiftUI

/// The system search on the home stack, where a thumb reaches it: iOS 26 puts it at the bottom on iPhone; before, it
/// stays shown under the title instead of scrolling away.
@MainActor
struct HomeSearch: ViewModifier {
    @Binding var text: String
    let prompt: String

    func body(content: Content) -> some View {
        #if os(iOS)
            if #available(iOS 26, *) {
                content.searchable(text: $text, prompt: prompt)
            } else {
                content.searchable(text: $text, placement: .navigationBarDrawer(displayMode: .always), prompt: prompt)
            }
        #else
            content.searchable(text: $text, prompt: prompt)
        #endif
    }
}
