// Waits for an uploaded build to finish processing, then gives it its "What to Test", adds it to an external
// TestFlight group and submits it to Beta App Review, through the App Store Connect API.
// Usage: swift testflight.swift <bundle-id> <version> <build> <group> <notes-file>
// with ANYWHERE_ASC_KEY (path to a .p8), ANYWHERE_ASC_KEY_ID and ANYWHERE_ASC_ISSUER.
import CryptoKit
import Foundation

let args = CommandLine.arguments
guard args.count == 6 else { fail("usage: testflight.swift <bundle-id> <version> <build> <group> <notes-file>") }
let (bundleID, version, build, groupName) = (args[1], args[2], args[3], args[4])
let notes = try String(contentsOfFile: args[5], encoding: .utf8)
let env = ProcessInfo.processInfo.environment
let key = try P256.Signing.PrivateKey(pemRepresentation: String(contentsOfFile: env["ANYWHERE_ASC_KEY"] ?? "", encoding: .utf8))

func fail(_ message: String) -> Never {
    FileHandle.standardError.write(Data("testflight: \(message)\n".utf8))
    exit(1)
}

func base64URL(_ data: Data) -> String {
    data.base64EncodedString().replacingOccurrences(of: "+", with: "-").replacingOccurrences(of: "/", with: "_")
        .replacingOccurrences(of: "=", with: "")
}

// A fresh token per call: the wait for processing outlasts the 20 minutes one may live.
func token() throws -> String {
    let now = Int(Date().timeIntervalSince1970)
    let header = try JSONSerialization.data(withJSONObject: ["alg": "ES256", "kid": env["ANYWHERE_ASC_KEY_ID"] ?? "", "typ": "JWT"])
    let claims = try JSONSerialization.data(withJSONObject: [
        "iss": env["ANYWHERE_ASC_ISSUER"] ?? "", "iat": now, "exp": now + 1200, "aud": "appstoreconnect-v1",
    ])
    let signed = base64URL(header) + "." + base64URL(claims)
    return signed + "." + base64URL(try key.signature(for: Data(signed.utf8)).rawRepresentation)
}

@discardableResult
func api(_ method: String, _ path: String, _ body: [String: Any]? = nil) async throws -> [String: Any] {
    var request = URLRequest(url: URL(string: "https://api.appstoreconnect.apple.com/v1/" + path)!)
    request.httpMethod = method
    request.setValue("Bearer \(try token())", forHTTPHeaderField: "Authorization")
    if let body {
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONSerialization.data(withJSONObject: body)
    }
    let (data, response) = try await URLSession.shared.data(for: request)
    let status = (response as! HTTPURLResponse).statusCode
    guard (200..<300).contains(status) else { fail("\(method) \(path): \(status) \(String(decoding: data, as: UTF8.self))") }
    return data.isEmpty ? [:] : (try JSONSerialization.jsonObject(with: data) as? [String: Any] ?? [:])
}

func list(_ path: String) async throws -> [[String: Any]] {
    try await api("GET", path)["data"] as? [[String: Any]] ?? []
}

func query(_ value: String) -> String {
    value.addingPercentEncoding(withAllowedCharacters: .alphanumerics) ?? value
}

guard let app = try await list("apps?filter[bundleId]=\(query(bundleID))").first?["id"] as? String else {
    fail("no app with bundle ID \(bundleID) in App Store Connect")
}

// The upload returns before App Store Connect lists the build, and processing takes minutes more.
var buildID: String?
for _ in 0..<90 {
    let found = try await list("builds?filter[app]=\(app)&filter[version]=\(query(build))&filter[preReleaseVersion.version]=\(query(version))")
    let attributes = found.first?["attributes"] as? [String: Any]
    switch attributes?["processingState"] as? String {
    case "VALID": buildID = found.first?["id"] as? String
    case "FAILED", "INVALID": fail("build \(version) (\(build)) failed processing")
    default: try await Task.sleep(for: .seconds(30))
    }
    if buildID != nil { break }
}
guard let buildID else { fail("build \(version) (\(build)) didn't finish processing in 45 minutes") }
let buildRef: [String: Any] = ["data": ["type": "builds", "id": buildID]]

let localizations = try await list("builds/\(buildID)/betaBuildLocalizations")
if let english = localizations.first(where: { ($0["attributes"] as? [String: Any])?["locale"] as? String == "en-US" }),
    let id = english["id"] as? String
{
    try await api("PATCH", "betaBuildLocalizations/\(id)", [
        "data": ["type": "betaBuildLocalizations", "id": id, "attributes": ["whatsNew": notes]],
    ])
} else {
    try await api("POST", "betaBuildLocalizations", [
        "data": ["type": "betaBuildLocalizations", "attributes": ["locale": "en-US", "whatsNew": notes], "relationships": ["build": buildRef]],
    ])
}

guard let group = try await list("betaGroups?filter[app]=\(app)&filter[name]=\(query(groupName))").first?["id"] as? String else {
    fail("no TestFlight group named \(groupName)")
}
try await api("POST", "betaGroups/\(group)/relationships/builds", ["data": [["type": "builds", "id": buildID]]])
try await api("POST", "betaAppReviewSubmissions", [
    "data": ["type": "betaAppReviewSubmissions", "relationships": ["build": buildRef]],
])
print("testflight: \(version) (\(build)) is in \(groupName) and waiting for Beta App Review")
