import Client
import Testing
import Wire
@testable import Stores

@Suite("Error text")
struct ErrorTextTests {
    @Test("each way a request fails reads as a sentence that names the Mac", arguments: [
        (DaemonError.offline, "Not connected to Studio"),
        (DaemonError.timedOut, "Studio didn't answer in time"),
        (DaemonError.closed, "The connection to Studio dropped"),
        (DaemonError.encoding("git.status"), "Studio sent a reply this app can't read"),
    ])
    func names(_ error: DaemonError, _ text: String) {
        #expect(ErrorText.describe(error, mac: "Studio") == text)
    }

    @Test("a refusal from the Mac shows its own message")
    func server() {
        let error = DaemonError.server(ServerError(message: "Branch is checked out elsewhere"))
        #expect(ErrorText.describe(error, mac: "Studio") == "Branch is checked out elsewhere")
    }

    @Test("a cancelled request has no message")
    func cancelled() {
        #expect(ErrorText.describe(DaemonError.cancelled, mac: "Studio") == nil)
    }

    @Test("an error from anywhere else never shows its Swift name")
    func other() {
        struct Odd: Error {}
        #expect(ErrorText.describe(Odd(), mac: "Studio") == "Something went wrong")
    }
}
