import DesignSystem
import SwiftUI
#if os(iOS)
    import AVFoundation
    import UIKit
#endif

/// The camera, looking for the QR code the Mac shows.
@MainActor
struct ScanView: View {
    let model: PairingModel
    @State private var notOurs = false
    @State private var denied = false

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            camera
            RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous)
                .strokeBorder(.white.opacity(0.85), lineWidth: 3)
                .frame(width: 240, height: 240)
                .accessibilityHidden(true)
        }
        .safeAreaInset(edge: .bottom) {
            VStack(spacing: Space.m) {
                Text(denied ? "Anywhere can't use the camera." : notOurs ? "That isn't an Anywhere pairing code." : "Point at the QR code on your Mac.")
                    .font(Typography.headline)
                    .foregroundStyle(notOurs ? Palette.failedText.color : Palette.text.color)
                    .multilineTextAlignment(.center)
                if denied {
                    settingsLink
                }
                Button("Enter code instead") { model.path = [.enter] }
                    .font(Typography.subheadline)
            }
            .padding(Space.l)
            .frame(maxWidth: .infinity)
            .glass(in: RoundedRectangle(cornerRadius: Radius.sheet, style: .continuous))
            .padding(Space.l)
        }
        .navigationTitle("Scan")
        .inlineTitle()
        .task { denied = !(await Self.cameraAllowed()) }
    }

    @ViewBuilder
    private var camera: some View {
        #if os(iOS)
            if !denied {
                QRScanner { text in
                    if model.open(text) {
                        Haptics.success()
                    } else if !notOurs {
                        notOurs = true
                        Haptics.warning()
                    }
                }
                .ignoresSafeArea()
            }
        #else
            Placeholder("Scanning needs the iPhone's camera", systemImage: "camera", detail: "Enter the code instead.")
                .environment(\.colorScheme, .dark)
        #endif
    }

    @ViewBuilder
    private var settingsLink: some View {
        #if os(iOS)
            Button("Allow in Settings") {
                if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
            }
            .buttonStyle(.borderedProminent)
        #endif
    }

    static func cameraAllowed() async -> Bool {
        #if os(iOS)
            switch AVCaptureDevice.authorizationStatus(for: .video) {
            case .authorized: return true
            case .notDetermined: return await AVCaptureDevice.requestAccess(for: .video)
            default: return false
            }
        #else
            return true
        #endif
    }
}

#if os(iOS)
    /// AVFoundation's QR reader behind a preview layer. It reports each new code once.
    struct QRScanner: UIViewRepresentable {
        let found: @MainActor (String) -> Void

        func makeUIView(context: Context) -> Preview {
            let view = Preview()
            view.found = found
            view.start()
            return view
        }

        func updateUIView(_ view: Preview, context: Context) {
            view.found = found
        }

        static func dismantleUIView(_ view: Preview, coordinator: ()) {
            view.stop()
        }

        final class Preview: UIView, AVCaptureMetadataOutputObjectsDelegate {
            override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

            var found: (@MainActor (String) -> Void)?
            private let session = AVCaptureSession()
            private var last: String?

            func start() {
                guard let device = AVCaptureDevice.default(for: .video), let input = try? AVCaptureDeviceInput(device: device),
                    session.canAddInput(input)
                else { return }
                session.addInput(input)
                let output = AVCaptureMetadataOutput()
                guard session.canAddOutput(output) else { return }
                session.addOutput(output)
                output.setMetadataObjectsDelegate(self, queue: .main)
                output.metadataObjectTypes = [.qr]
                if let preview = layer as? AVCaptureVideoPreviewLayer {
                    preview.session = session
                    preview.videoGravity = .resizeAspectFill
                }
                // startRunning blocks until the camera is up, so it stays off the main thread.
                nonisolated(unsafe) let session = session
                DispatchQueue.global(qos: .userInitiated).async { session.startRunning() }
            }

            func stop() {
                nonisolated(unsafe) let session = session
                DispatchQueue.global(qos: .userInitiated).async { session.stopRunning() }
            }

            nonisolated func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput objects: [AVMetadataObject], from connection: AVCaptureConnection) {
                let text = objects.compactMap { ($0 as? AVMachineReadableCodeObject)?.stringValue }.first
                // The delegate queue is main.
                MainActor.assumeIsolated {
                    guard let text, text != last else { return }
                    last = text
                    found?(text)
                }
            }
        }
    }
#endif
