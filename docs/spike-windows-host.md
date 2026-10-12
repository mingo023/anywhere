# Spike: pocketd on Windows

Can pocketd be built for Windows from this Mac, and how does the desktop reach it? Run 2026-10-12 against ghostty `4ae9f1a`, Go 1.27.1, zig 0.15.

## (a) libghostty-vt for Windows: yes

`scripts/build-ghostty.sh windows-amd64` and `windows-arm64` build with `zig build -Demit-lib-vt -Dtarget=<arch>-windows-gnu` into `third_party/ghostty/zig-out-windows-<arch>/` (about 2.5 min amd64, 1 min arm64 once the zig cache is warm).

Output differs from Linux: `lib/ghostty-vt-static.lib` (static), `lib/ghostty-vt.lib` + `bin/ghostty-vt.dll` (dynamic). The static `.pc` lists no private libs.

## (b) Go cgo cross-compile: yes

```sh
GOOS=windows GOARCH=amd64 CGO_ENABLED=1 CC="zig cc -target x86_64-windows-gnu" go test -c ./internal/vt
GOOS=windows GOARCH=arm64 CGO_ENABLED=1 CC="zig cc -target aarch64-windows-gnu" go test -c ./internal/vt
```

Both link to PE32+ executables. Two things were needed in `internal/vt/vt.go`:

- `-DGHOSTTY_STATIC`, or the header declares every symbol `__declspec(dllimport)` and lld rejects the static lib ("not an import library").
- `-L<dir> -lghostty-vt-static` rather than the file path: cgo's LDFLAGS allowlist rejects `.lib` paths.

Nothing was run on Windows; there is no Windows machine here.

## (c) Unix socket forwarding through Win32-OpenSSH: no

Win32-OpenSSH does not support unix domain sockets in forwarding or `SSH_AUTH_SOCK`:

- [PowerShell/Win32-OpenSSH#1564](https://github.com/PowerShell/Win32-OpenSSH/issues/1564): `RemoteForward` of a unix socket reports success, then fails in `w32_getsockopt`.
- [PowerShell/Win32-OpenSSH#1761](https://github.com/PowerShell/Win32-OpenSSH/issues/1761): unix sockets are unsupported for `SSH_AUTH_SOCK`; [microsoft/WSL#7050](https://github.com/microsoft/WSL/issues/7050) was redirected there.
- [skreutz.com](https://www.skreutz.com/posts/unix-domain-socket-forwarding-with-openssh/): the OpenSSH shipped with Windows 10 cannot forward unix sockets.

So on Windows pocketd listens on `127.0.0.1:<port>` (4519 release, 4520 dev) and the desktop forwards that TCP port with `ssh -L`. Any local user can open a loopback port, so a TCP peer is never owner by pid; it must present an owner token minted by `pocketd pair --json` over SSH.

## Verdict

Go ahead. What can't be checked here: ConPTY behaviour, Task Scheduler install, PowerShell bootstrap, and job-object kill. They are built and unit-tested by cross-compilation and need a run on a real Windows host.
