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
    /// nil until the camera permission is known; the camera starts only once it's granted.
    @State private var allowed: Bool?

    init(model: PairingModel, allowed: Bool? = nil) {
        self.model = model
        _allowed = State(initialValue: allowed)
    }

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            if denied {
                cameraOff
            } else {
                camera
                Viewfinder(hint: notOurs ? "That isn't a Zoes pairing code." : "Point at the QR code on your Mac.", failed: notOurs)
            }
        }
        .safeAreaInset(edge: .bottom) {
            Button("Enter code instead") { model.path = [.enter] }
                .buttonStyle(ScanButtonStyle(denied ? .plain : .glass))
                .padding(.bottom, Space.m)
        }
        .environment(\.colorScheme, .dark)
        .pairingBar()
        .navigationBarBackButtonHidden(true)
        .toolbar { ToolbarItem(placement: .cancellationAction) { CloseButton(tint: .fixed(0xffffffff)) } }
        .task { if allowed == nil { allowed = await Self.cameraAllowed() } }
    }

    private var denied: Bool { allowed == false }

    private var cameraOff: some View {
        VStack(spacing: 0) {
            LucideIcon(.cameraOff, size: 40, weight: 1.75).foregroundStyle(Token.fixed(0x818181ff).color)
            Text("Camera access is off").font(.system(size: 20, weight: .semibold)).foregroundStyle(.white).padding(.top, 18)
            Text("Zoes needs the camera to scan the pairing code on your Mac.")
                .font(.system(size: 15)).foregroundStyle(Token.fixed(0xa1a1a1ff).color)
                .multilineTextAlignment(.center)
                .padding(.top, Space.s)
            Button("Allow in Settings", action: Self.openSettings)
                .buttonStyle(ScanButtonStyle(.light))
                .padding(.top, Space.xl)
        }
        .padding(.horizontal, 36)
        .offset(y: -60)
    }

    @ViewBuilder
    private var camera: some View {
        #if os(iOS)
            if allowed == true {
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
            Text("camera feed").font(.system(size: 11, design: .monospaced)).foregroundStyle(Token.fixed(0x6c6c6cff).color)
                .frame(maxHeight: .infinity, alignment: .top).padding(.top, 200)
        #endif
    }

    private static func openSettings() {
        #if os(iOS)
            if let url = URL(string: UIApplication.openSettingsURLString) { UIApplication.shared.open(url) }
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
                let text = objects.compactMap { ($0 as? AVMetadataMachineReadableCodeObject)?.stringValue }.first
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
