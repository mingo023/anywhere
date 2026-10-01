package main

import "runtime/debug"

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
