import Foundation

/// Answers URLSession requests in-process, as the push relay would.
public final class RelayStub: URLProtocol, @unchecked Sendable {
    public typealias Answer = @Sendable (URLRequest) throws -> (Int, String)
    nonisolated(unsafe) static var answers: [String: Answer] = [:]
    static let lock = NSLock()

    public static func session(_ answer: @escaping Answer) -> URLSession {
        let id = UUID().uuidString
        lock.withLock { answers[id] = answer }
        let config = URLSessionConfiguration.ephemeral
        config.protocolClasses = [RelayStub.self]
        config.httpAdditionalHeaders = ["X-Stub": id]
        return URLSession(configuration: config)
    }

    public static func body(_ request: URLRequest) -> Data {
        if let body = request.httpBody { return body }
        guard let stream = request.httpBodyStream else { return Data() }
        stream.open()
        defer { stream.close() }
        var data = Data()
        var buffer = [UInt8](repeating: 0, count: 4096)
        while stream.hasBytesAvailable {
            let n = stream.read(&buffer, maxLength: buffer.count)
            if n <= 0 { break }
            data.append(buffer, count: n)
        }
        return data
    }

    override public class func canInit(with request: URLRequest) -> Bool { true }
    override public class func canonicalRequest(for request: URLRequest) -> URLRequest { request }

    override public func startLoading() {
        let id = request.value(forHTTPHeaderField: "X-Stub") ?? ""
        let answer = Self.lock.withLock { Self.answers[id] }
        do {
            let (status, body) = try answer?(request) ?? (500, "")
            let response = HTTPURLResponse(url: request.url!, statusCode: status, httpVersion: "HTTP/1.1", headerFields: ["Content-Type": "application/json"])!
            client?.urlProtocol(self, didReceive: response, cacheStoragePolicy: .notAllowed)
            client?.urlProtocol(self, didLoad: Data(body.utf8))
            client?.urlProtocolDidFinishLoading(self)
        } catch {
            client?.urlProtocol(self, didFailWithError: error)
        }
    }

    override public func stopLoading() {}
}
