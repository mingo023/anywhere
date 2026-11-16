package hosts

import (
	"encoding/json"
	"reflect"
	"slices"
	"testing"

	"pocketd/internal/proto"
)

func asMap(t *testing.T, v any) map[string]any {
	t.Helper()
	raw, err := json.Marshal(v)
	if err != nil {
		t.Fatal(err)
	}
	var m map[string]any
	if err := json.Unmarshal(raw, &m); err != nil {
		t.Fatal(err)
	}
	return m
}

func TestATargetWritesTheSameOnTheWireAsInTheHostsFile(t *testing.T) {
	for _, target := range []Target{
		{Kind: KindAlias, Name: "devbox"},
		{Kind: KindAddress, Addr: "10.0.4.12", Port: 22},
		{Kind: KindAddress, Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/id_ed25519"},
	} {
		wire := proto.HostTarget{Kind: target.Kind, Name: target.Name, Addr: target.Addr, Port: target.Port, User: target.User, KeyFile: target.KeyFile}
		if got, want := asMap(t, wire), asMap(t, target); !reflect.DeepEqual(got, want) {
			t.Errorf("%+v: wire %v, file %v", target, got, want)
		}
	}
}

func TestAHostInTheHostsFileReadsAsAWireHostBeforeItsSocketAndLink(t *testing.T) {
	for _, host := range []Host{
		{ID: "devbox", Name: "devbox", Target: Target{Kind: KindAlias, Name: "devbox"}},
		{ID: "10-0-4-12", Name: "10.0.4.12", Windows: true, Target: Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "k"}},
	} {
		raw, err := json.Marshal(host)
		if err != nil {
			t.Fatal(err)
		}
		var wire proto.Host
		if err := json.Unmarshal(raw, &wire); err != nil {
			t.Fatal(err)
		}
		wire.Link = proto.HostLinkState{State: proto.LinkConnecting}
		got := asMap(t, wire)
		delete(got, "socket")
		delete(got, "link")
		if want := asMap(t, host); !reflect.DeepEqual(got, want) {
			t.Errorf("wire %v, file %v", got, want)
		}
	}
}

func TestEveryFailureAndLinkStateTheHostsPackageNamesIsOnTheWire(t *testing.T) {
	failures := []Failure{FailAuth, FailUnknownKey, FailKeyChanged, FailNoSuchHost, FailRefused, FailTimedOut, FailDropped, FailThrottled, FailNotRunning, FailUnpaired, FailPathTooLong, FailOther}
	var named []string
	for _, f := range failures {
		named = append(named, string(f))
	}
	slices.Sort(named)
	if want := slices.Sorted(slices.Values(proto.HostFailures)); !slices.Equal(named, want) {
		t.Errorf("hosts name %v, wire %v", named, want)
	}
	for state, want := range map[LinkState]string{StateConnecting: proto.LinkConnecting, StateOnline: proto.LinkOnline, StateRetrying: proto.LinkRetrying, StateFailed: proto.LinkFailed} {
		if string(state) != want {
			t.Errorf("state %q, wire %q", state, want)
		}
	}
}

func TestTheInstallStepsAndProgressKindsAreTheWordsOnTheWire(t *testing.T) {
	words := map[Step]string{StepDetect: "detect", StepDownload: "download", StepVerify: "verify", StepUnpack: "unpack", StepLink: "link", StepService: "service", StepStart: "start"}
	for step, want := range words {
		if string(step) != want {
			t.Errorf("step %q, wire %q", step, want)
		}
	}
	if len(words) != len(steps) {
		t.Errorf("the wire names %d steps, the package %d", len(words), len(steps))
	}
	kinds := map[ProgressKind]string{ProgressStep: "step", ProgressWarn: "warn", ProgressRunning: "running", ProgressDone: "done", ProgressError: "error"}
	for kind, want := range kinds {
		if string(kind) != want {
			t.Errorf("kind %q, wire %q", kind, want)
		}
	}
}
