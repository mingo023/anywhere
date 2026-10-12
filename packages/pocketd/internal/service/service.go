// Package service runs a standalone pocketd as a per-user service on a host:
// a LaunchAgent on macOS, a systemd user unit on Linux.
package service

import (
	"bytes"
	"encoding/xml"
	"fmt"
	"path/filepath"
	"strings"
)

// Spec names the service: Exe is pocketd's absolute path, Home its POCKET_HOME, Label the service name.
type Spec struct{ Exe, Home, Label string }

// The app's agent policy (internal/launchagent) plus POCKET_HOME, since a host
// install may use a home other than the binary's default.
const plistTemplate = `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>%[1]s</string>
	<key>ProgramArguments</key>
	<array>
		<string>%[2]s</string>
		<string>serve</string>
	</array>
	<key>EnvironmentVariables</key>
	<dict>
		<key>POCKET_HOME</key>
		<string>%[3]s</string>
	</dict>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ThrottleInterval</key>
	<integer>30</integer>
	<key>ProcessType</key>
	<string>Interactive</string>
	<key>StandardOutPath</key>
	<string>%[4]s</string>
	<key>StandardErrorPath</key>
	<string>%[4]s</string>
</dict>
</plist>
`

func escape(s string) string {
	var b bytes.Buffer
	xml.EscapeText(&b, []byte(s))
	return b.String()
}

func logPath(s Spec) string { return filepath.Join(s.Home, "logs", "launchd.log") }

func plist(s Spec) []byte {
	return fmt.Appendf(nil, plistTemplate, escape(s.Label), escape(s.Exe), escape(s.Home), escape(logPath(s)))
}

// A pocketd already holding the home's lock makes a new one exit 75, so the unit
// retries until that one exits; with a start limit systemd would give up first.
const unitTemplate = `[Unit]
Description=Anywhere pocketd
StartLimitIntervalSec=0

[Service]
ExecStart=%s serve
Environment=%s
Restart=always
RestartSec=30

[Install]
WantedBy=default.target
`

// quote makes s one systemd word; % starts a specifier there.
func quote(s string) string {
	return `"` + strings.NewReplacer(`\`, `\\`, `"`, `\"`, "%", "%%").Replace(s) + `"`
}

func unit(s Spec) []byte {
	return fmt.Appendf(nil, unitTemplate, quote(s.Exe), quote("POCKET_HOME="+s.Home))
}
