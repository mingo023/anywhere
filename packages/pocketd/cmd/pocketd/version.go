package main

import (
	"encoding/json"
	"runtime"
	"runtime/debug"
)

// version is set by -ldflags "-X main.version=…"; otherwise the VCS stamp names the build.
var version string

func versionString() string {
	if version != "" {
		return version
	}
	info, ok := debug.ReadBuildInfo()
	if !ok {
		return "dev"
	}
	return fromSettings(info.Settings)
}

// versionJSON is what a desktop reads to pick the archive for this host.
func versionJSON() []byte {
	out, _ := json.Marshal(map[string]string{"version": versionString(), "os": runtime.GOOS, "arch": runtime.GOARCH})
	return out
}

func fromSettings(settings []debug.BuildSetting) string {
	rev, dirty := "", false
	for _, s := range settings {
		switch s.Key {
		case "vcs.revision":
			rev = s.Value
		case "vcs.modified":
			dirty = s.Value == "true"
		}
	}
	if len(rev) < 12 {
		return "dev"
	}
	if dirty {
		return rev[:12] + "-dirty"
	}
	return rev[:12]
}
