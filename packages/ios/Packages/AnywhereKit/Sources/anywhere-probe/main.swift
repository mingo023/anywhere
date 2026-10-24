import Client
import Foundation
import Stores
import Wire

// Drives a real pocketd through Client and Stores, as the app would, and prints PASS/FAIL/SKIP per check.
let usage = """
    usage: anywhere-probe --host <ip:port> (--token <device token> | --pair-code <code> | --owner-socket <path>)
                          [--project <path>] [--changed <relative file>] [--claude] [--timeout <s>]

      --token          say hello with a device token pairing gave earlier
      --pair-code      pair with a code the Mac shows, then say hello
      --owner-socket   open a code on pocketd's owner socket (as the desktop does) and pair with it
      --project        a registered project the git, fs and workspace checks use
      --changed        a file with uncommitted changes in --project, for git.diff
      --claude         start a claude in --project over the owner socket, then prompt it and read its timeline
    Exits 0 when nothing failed.
    """

struct Options {
    var host = ""
    var token: String?
    var pairCode: String?
    var ownerSocket: String?
    var project: String?
    var changed: String?
    var claude = false
    var timeout: Double = 15

    init(_ args: [String]) throws {
        var rest = args[...]
        func value(_ flag: String) throws -> String {
            guard let v = rest.popFirst() else { throw Probe.Failure("\(flag) needs a value") }
            return v
        }
        while let flag = rest.popFirst() {
            switch flag {
            case "--host": host = try value(flag)
            case "--token": token = try value(flag)
            case "--pair-code": pairCode = try value(flag)
            case "--owner-socket": ownerSocket = try value(flag)
            case "--project": project = try value(flag)
            case "--changed": changed = try value(flag)
            case "--claude": claude = true
            case "--timeout": timeout = Double(try value(flag)) ?? timeout
            default: throw Probe.Failure("unknown flag \(flag)")
            }
        }
        guard !host.isEmpty, token != nil || pairCode != nil || ownerSocket != nil else { throw Probe.Failure("--host and a way in are needed") }
        if claude, ownerSocket == nil || project == nil { throw Probe.Failure("--claude needs --owner-socket and --project") }
    }
}

@MainActor
func main() async -> Int32 {
    let options: Options
    do {
        options = try Options(Array(CommandLine.arguments.dropFirst()))
    } catch {
        print("\(error)\n\n\(usage)")
        return 2
    }
    var owner: OwnerSocket?
    var credentials: Credentials

    // Pairing goes through the app's own path: AppModel.pair → Pairer → MemoryVault.
    if let token = options.token {
        credentials = Credentials(host: options.host, deviceToken: token, deviceId: "probe", macName: options.host, clientId: Credentials.newClientId(os: "probe"))
    } else {
        var code = options.pairCode ?? ""
        if let path = options.ownerSocket {
            do {
                owner = try OwnerSocket(path: path)
                code = try owner!.beginPairing(host: options.host).code
                print("PASS owner pair.begin — code \(code.prefix(4))…")
            } catch {
                print("FAIL owner pair.begin — \(error)")
                return 1
            }
        }
        let vault = MemoryVault()
        let app = AppModel(vault: vault)
        do {
            let start = Date()
            let host = try await app.pair(PairingLink(host: options.host, code: code, name: options.host), phoneName: "anywhere-probe")
            credentials = host.credentials
            app.forget(host.id)
            print("PASS pair — device \(credentials.deviceId), token saved (\(Int(Date().timeIntervalSince(start) * 1000)) ms)")
        } catch {
            print("FAIL pair — \(error)")
            return 1
        }
        if let replay = try? await Pairer.pair(host: options.host, code: code, phoneName: "anywhere-probe") {
            print("FAIL a spent code is refused — it paired \(replay.deviceId) again")
        } else {
            print("PASS a spent code is refused")
        }
    }

    let probe = Probe(credentials: credentials)
    do {
        try await probe.until("hello.ok", seconds: options.timeout) { probe.host.link == .online || probe.host.reachability(at: .now).isFinal }
    } catch {
        print("FAIL connect — \(error); link \(probe.host.link)")
        return 1
    }
    guard probe.host.link == .online else {
        print("FAIL connect — \(probe.host.reachability(at: .now).label)")
        return 1
    }

    var setup = Setup(project: options.project, changedFile: options.changed)
    if options.claude, let owner, let project = options.project {
        do {
            setup.claudeTerminal = try owner.spawn("claude", cwd: project, env: ProcessInfo.processInfo.environment.map { "\($0)=\($1)" })
            print("PASS owner spawn claude — terminal \(setup.claudeTerminal!)")
        } catch {
            print("FAIL owner spawn claude — \(error)")
            return 1
        }
    }

    await probe.runAll(setup)

    // Last, since it ends the connection: the Mac revokes this phone, and the app keeps the host but drops the token.
    if let owner {
        let vault = MemoryVault([credentials])
        let app = AppModel(vault: vault)
        app.add(probe.host, start: false)
        await probe.check("a revoked phone ends its link and forgets its token") {
            try owner.revoke(deviceId: credentials.deviceId)
            try await probe.until("the 4401 close") { probe.host.reachability(at: .now) == .revoked }
            try await probe.until("the token wiped") { (try? vault.all().first?.deviceToken) == "" }
            return "link \(probe.host.reachability(at: .now).label), host \(try vault.all().first?.host ?? "-") kept"
        }
    }
    await probe.stop()
    print("\(probe.passed) passed, \(probe.failed) failed, \(probe.skipped) skipped")
    _ = owner
    return probe.failed == 0 ? 0 : 1
}

exit(await main())
