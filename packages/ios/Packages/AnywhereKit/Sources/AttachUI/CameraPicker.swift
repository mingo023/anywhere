#if os(iOS)
    import SwiftUI
    import UIKit

    /// The camera, full screen; `took` gets the photo, and the sheet closes either way.
    @MainActor
    struct CameraPicker: UIViewControllerRepresentable {
        let took: (UIImage) -> Void
        @Environment(\.dismiss) private var dismiss

        func makeUIViewController(context: Context) -> UIImagePickerController {
            let picker = UIImagePickerController()
            picker.sourceType = .camera
            picker.delegate = context.coordinator
            return picker
        }

        func updateUIViewController(_ picker: UIImagePickerController, context: Context) {}

        func makeCoordinator() -> Coordinator { Coordinator(self) }

        @MainActor
        final class Coordinator: NSObject, UIImagePickerControllerDelegate, UINavigationControllerDelegate {
            let parent: CameraPicker

            init(_ parent: CameraPicker) { self.parent = parent }

            func imagePickerController(_ picker: UIImagePickerController, didFinishPickingMediaWithInfo info: [UIImagePickerController.InfoKey: Any]) {
                if let image = info[.originalImage] as? UIImage { parent.took(image) }
                parent.dismiss()
            }

            func imagePickerControllerDidCancel(_ picker: UIImagePickerController) { parent.dismiss() }
        }
    }
#endif
