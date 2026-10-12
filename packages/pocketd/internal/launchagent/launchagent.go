// Package launchagent installs pocketd as a per-user launchd service.
package launchagent

import (
	"bytes"
	"encoding/xml"
	"fmt"
	"path/filepath"

	"pocketd/internal/config"
)

type Spec struct{ Exe, LogPath string }

// KeepAlive restarts only a failed exit, so a clean stop stays stopped.
// Interactive keeps launchd from throttling the PTYs' CPU and I/O.
const plist = `<?xml version="1.0" encoding="UTF-8"?>
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
	<string>%[3]s</string>
	<key>StandardErrorPath</key>
	<string>%[3]s</string>
</dict>
</plist>
`

func escape(s string) string {
	var b bytes.Buffer
	xml.EscapeText(&b, []byte(s))
	return b.String()
}

func Plist(s Spec) []byte {
	return fmt.Appendf(nil, plist, config.Label(), escape(s.Exe), escape(s.LogPath))
}

func Path(home string) string {
	return filepath.Join(home, "Library", "LaunchAgents", config.Label()+".plist")
}
