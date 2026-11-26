# Windows regression findings

A regression of Anywhere on Windows 11 (x86_64, GNU target), covering the desktop, pocketd, its CLI, and remote control (hosts, phone pairing, the owner port). It was run from the `claude/windows-regression-testing` worktree on 2026-11-26. CI runs no Go or Rust tests on Windows, so these are the first Windows runs of either suite.

Status: **fixed**, **open** (left for a follow-up, with the reason), **by design**.

## Build and checkout

| # | Finding | Status |
|---|---|---|
| B1 | With `core.autocrlf=true` (Git for Windows' default), shell scripts and golden files check out with CRLF, which breaks `scripts/*.sh` and the protocol golden tests. `.gitattributes` now pins `eol=lf`. | fixed |
| B2 | `crates/pocket/build.rs` had `zig rc` emit a `.res`, which only lld links; a native `x86_64-pc-windows-gnu` build links with GNU ld and failed with "app.res: file not recognized". It now emits a COFF object, which both link. | fixed |

## pocketd

| # | Finding | Status |
|---|---|---|
| P1 | `logfile.rotate` renamed the log while it was still open, which Windows refuses, and then reopened it truncating, wiping the live log on every rotation. It now closes first and appends when the rename fails. | fixed |
| P2 | `gitview.Top`, `worktree.parse` and `launch.revParse` compared git's `C:/x` output with `filepath`'s `C:\x`, so a repo's own worktree never matched. | fixed |
| P3 | Path containment (`fsview.within`, `worktree.Containing`, `inside`, `project`, `Claim`, `AgentMade`) checked a `/` prefix, so on Windows no path was ever inside a project. | fixed |
| P4 | `launch.trustedAt` read only `HOME`, which Windows doesn't set, so claude's trust file was never found. It falls back to `USERPROFILE`. | fixed |
| P5 | `accounts` shares used `os.Symlink`, which needs Developer Mode or admin on Windows. It falls back to a junction for folders and a hard link for files. | fixed |
| P6 | `anywhere.exe` wasn't recognised as the `anywhere` command (main.go). | fixed |
| P7 | `pipe.go` treated `C:/x/claude.exe` and `.\claude` as names to look up on PATH. | fixed |
| P8 | `shellenv.IsExecutable` needs a unix exec bit, which Go never reports on Windows, so `config set claude.command C:\…\claude.exe` was always refused. It now accepts a regular file whose extension is in `PATHEXT`. | fixed |
| P9 | `atomicfile.Write` fails its rename with "Access is denied" while another reader has the target open, so a CLI `owner revoke` can fail while pocketd is reading the owners file. On Windows the rename retries for up to a second on a sharing or access error. | fixed |
| P10 | `upgrade` refuses on Windows (`errNoHandoff`), since a lock can't be kept across exec. | by design |
| P11 | On Windows the owner port defaults to `127.0.0.1` TCP (docs/spike-windows-host.md), so `owner pair` mints a token where macOS would not. | by design |
| P12 | Every terminal pocketd started, and every agent launched in one, showed a blank screen and ignored typing whenever pocketd's own stdio was redirected (a log file, or a service). `conpty.create` didn't set `STARTF_USESTDHANDLES`, so the child inherited pocketd's handles instead of the pseudoconsole's. | fixed |
| P13 | The CLI commands only the owner may send (`devices`, `pair`, `run`, `upgrade`, `config set` while pocketd runs) were refused with "not the owner": on Windows no pid proves the owner. The CLI now authenticates with an owner token it keeps in `cli.token` in home, minting one the first time and again after a revoke, as the desktop does with `desktop.token`. | fixed |
| P14 | Windows PowerShell 5.1 drops the double quotes inside an argument it passes to a native program, so a login spawn or an agent launch whose argv held `"` (`sh -c 'echo "x"'`, a prompt with quotes) reached the program broken. `shellenv.PSCommandIn` escapes them for `powershell.exe`; login spawns, `Wrap`, `InShell` and account launches use it. pwsh 7.3+ passes them intact and is left alone. | fixed |
| P15 | Windows PowerShell still splits an argument holding an odd number of `"` followed by a space, and pwsh 7.0–7.2 strips quotes the way 5.1 does. | open: rare, and the fix needs version detection |
| P16 | `launch` resolved an agent's command with `exec.LookPath`, which skips npm's `claude.cmd` shim; it now uses `terminal.LookPath`, which honours `PATHEXT`. `bare()` also handles `\` and drive letters. | fixed |

## Remote control

| # | Finding | Status |
|---|---|---|
| R1 | `reach.IsTailnet` only knew the macOS and Linux interface names, so on Windows a Tailscale 100.x address was taken for carrier-grade NAT and the phone was told "Phone access needs Tailscale". It now recognises the adapter named `Tailscale`. | fixed |
| R2 | Phone pairing, device listing, revoke, the owner port, `pair --json`, config, files, the host op and terminals driven over the socket pass end to end on Windows (the `e2e` files ported in this pass). The rest of `e2e` (chat, delegation, hosts, board, automation, upgrade) drives `sh` fakes and stays `unix`-only, so those journeys have only unit-test coverage on Windows. | works, partly untested |
| R3 | A Windows desktop can't reach a remote host over ssh: `hosts/ssh.go` builds `-L <local socket>:<remote socket>`, the drive letter's colon splits that spec, and Win32-OpenSSH has no unix-socket forwarding. It needs a TCP local end across `hosts/ssh.go`, `hosts.go`, `link.go`, `proto.Host.Socket` and the desktop's `hosts.rs` and `link.rs`. | open: design change |
| R4 | `hostop.Trash` has no Recycle Bin on Windows: it moves into its fallback folder by rename, which fails when that folder is on another drive. | open: needs the shell API |
| R5 | `UserKnownHostsFile` breaks on a UNC home path. | open |
| R6 | `config.ValidCommand` told a Windows user the command must be "a full path starting with /". | fixed |

## Desktop

| # | Finding | Status |
|---|---|---|
| D1 | `daemon::login_command` started an agent CLI with `Command::new`, which tries only `.exe`, so npm's `claude.cmd` shim was never found. It now looks the program up on the session's PATH by `PATHEXT` (`which`). | fixed |
| D2 | The home folder came from `HOME`, which Windows doesn't set, so `~` was never shortened and SSH aliases were read from nowhere. `form::home`, `util::tilde` and the fleet form use `std::env::home_dir`. | fixed |
| D3 | `util::folder`, `tilde_in` and `untilde_in` split only on `/`, so a Windows path's folder name was the whole path. | fixed |
| D4 | A link or file reference in chat joined the agent's folder with `Path::join`, which on Windows puts `\` into a unix host's path. `util::join` uses the separator the folder is written with. | fixed |
| D5 | `git::Runner::place` kept Git for Windows' `C:/x`, so a worktree's path never matched the project's `C:\x`. | fixed |
| D6 | The Windows terminal default handed `-NoLogo` to any shell and `-l` to none. The argument now follows the shell: PowerShell gets `-NoLogo`, others `-l`. | fixed |
| D7 | Windows' AF_UNIX sockets ignore `SO_SNDTIMEO`, so a send to a pocketd that stopped reading hung the desktop for good. Writes now run on their own thread and give up at the timeout, and the reader wakes every 100ms to notice. A write thread stuck that way stays blocked until pocketd reads or closes the socket. | fixed |
| D8 | A Codex transcript's relative path was sent to the host with `\`. | fixed |
| D9 | Sounds play with `afplay`, which exists only on macOS. They are skipped elsewhere; playing them on Windows needs an audio crate for the mp3s. | open: needs a player |
| D10 | Trash in the Explorer has no Recycle Bin on Windows (same gap as R4). | open: needs the shell API |
| D11 | The local machine is labelled "This Mac" on every OS (sidebar, settings, spaces). | open: naming is a product call |
| D12 | A `cargo build` of the GNU target leaves `WebView2Loader.dll` under `target/…/build/webview2-com-sys-*/out/x64`, so the dev binary exits 127 until that folder is on PATH. `scripts/release-desktop.sh` already ships the DLL beside the exe. | by design |

Test-only: the agents link tests used `/tmp` for sockets, the scratch git repos ran with `core.autocrlf=true`, handoff's scratch dirs were canonicalized to `\\?\C:\…` (which git reads as a host), the plist test is macOS-only, and the mermaid emoji check accepted only Apple's pure red; Windows 11 draws ❌ in a pink-red (#fc628c), so it now asks for a strongly red pixel, which a monochrome placeholder still fails. Four handoff tests stand a local `sh` in for the unix host; on Windows that is Git Bash, whose paths are MSYS's (`/tmp/…`), so they are ignored there with that reason.

## Test-only portability

About 260 Go tests failed on Windows only because of their fixtures: `/repo`-style paths that aren't absolute on Windows, raw backslashes inside expected JSON, unix file modes, `#!/bin/sh` fakes, and `/tmp`. They now build their paths from the temp dir, or skip with a reason where the behaviour is unix-only. The `e2e` package and `internal/daemon` tests drive `sh` fakes, so their remaining files carry `//go:build unix`, and the helpers only they call moved into `harness_unix_test.go` so staticcheck stays clean on Windows.

Four tests failed during the Windows runs without a product bug behind them:

- `state.TestWriterSkipsUnchangedSnapshots` polled the file with `Load`, which renames a file it can't read to `.bad`. A read that lands during the writer's rename lost the snapshot, which the writer never writes again while it is unchanged. The test now reads the file directly. pocketd calls `Load` only at startup, before the writer runs.
- `wsserver.TestAClosedAgentsWorktreeGoesOnceTheLastShellInItCloses` returned while git was still deleting the branch, and the temp dir's cleanup failed on the files git held open. It now waits for the branch to go.
- `orch.TestAPromptSentBeforeTheSinkRecordedTheTurnBeforeAsEndedStillOpensAFollowUp` and `launch.TestACommandNamedAsAnAgentIsItOnlyWhenItsHelpSaysSo` each failed once while the whole suite ran and pass alone; both are timing under load (the second waits on a `--help` probe), not Windows behaviour.

## How to rerun

pocketd and pushrelay, from Git Bash with Go and [zig](https://ziglang.org) on PATH (cgo needs a C compiler):

```sh
cd packages/pocketd
CGO_ENABLED=1 CC="zig cc -target x86_64-windows-gnu" go test -count=1 ./...
```

The desktop, on the `x86_64-pc-windows-gnu` toolchain. Two linker problems decide the setup. Rust's own MinGW objects crash at startup when mixed with llvm-mingw's C runtime, so C code must be built with GCC ([WinLibs](https://winlibs.com)). GNU ld then can't resolve zig's weak `___chkstk_ms` in `libghostty-vt.a`, so GCC has to drive lld. lld refuses the second manifest resource that GCC's `default-manifest.o` adds next to gpui's, hence `/force:multipleres`. The linker wrapper, `gcc-link.cmd`:

```bat
@"<winlibs>\bin\gcc.exe" -fuse-ld=lld "-B<llvm-mingw>\bin" -Wl,--Xlink=/force:multipleres %*
```

```sh
export CC_x86_64_pc_windows_gnu=<winlibs>/bin/gcc.exe CXX_x86_64_pc_windows_gnu=<winlibs>/bin/g++.exe AR_x86_64_pc_windows_gnu=<winlibs>/bin/ar.exe
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER='<path>\gcc-link.cmd'
cd packages/desktop && cargo test --workspace --no-fail-fast
```
