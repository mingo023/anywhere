package hosts

import (
	"errors"
	"strings"
	"testing"
)

func address(addr string) Target {
	return Target{Kind: KindAddress, Addr: addr, Port: 22}
}

func alias(name string) Target { return Target{Kind: KindAlias, Name: name} }

func TestAHostIsNamedForItsAliasOrTheFirstPartOfItsAddress(t *testing.T) {
	for target, want := range map[Target]string{
		alias("devbox"):                  "devbox",
		address("gpu-1.lab.example.com"): "gpu-1",
		address("10.0.4.12"):             "10.0.4.12",
	} {
		if got := New(target, nil).Name; got != want {
			t.Errorf("%+v is named %q, want %q", target, got, want)
		}
	}
}

func TestASecondHostWithTheSameNameGetsItsOwnID(t *testing.T) {
	first := New(address("devbox.a.com"), nil)
	second := New(address("devbox.b.com"), []Host{first})
	if first.ID != "devbox" || second.ID != "devbox-2" {
		t.Fatalf("%q, %q", first.ID, second.ID)
	}
}

func TestAHostsIDKeepsOnlyLowercaseLettersAndDigitsAtMost24OfThem(t *testing.T) {
	long := New(alias("A-Really-Long-Host-Alias-Somebody-Typed.internal"), nil).ID
	if long != "a-really-long-host-alias" {
		t.Errorf("%q", long)
	}
	for name, want := range map[string]string{"\u00e9": "host", "---": "host", "My Box!": "my-box", "caf\u00e9x": "caf-x"} {
		if got := New(alias(name), nil).ID; got != want {
			t.Errorf("%q has id %q, want %q", name, got, want)
		}
	}
}

func TestALongNameStillFitsAUnixSocketPath(t *testing.T) {
	host := New(alias("a-really-long-host-alias-somebody-typed-into-their-ssh-config.internal.example.com"), nil)
	paths := Paths{Home: "/Users/someone-with-a-long-username/.coding-pocket-dev"}
	if sock := paths.Socket(host.ID); len(sock) >= 104 {
		t.Fatalf("%d bytes: %s", len(sock), sock)
	}
}

func TestAHostsFilesSitUnderTheAppsHostsFolder(t *testing.T) {
	paths := Paths{Home: "/h"}
	got := []string{paths.Socket("dev"), paths.Token("dev"), paths.KnownHosts("dev"), paths.Key("dev")}
	want := []string{"/h/hosts/dev.sock", "/h/hosts/dev.token", "/h/hosts/dev.known_hosts", "/h/hosts/dev.key"}
	if strings.Join(got, " ") != strings.Join(want, " ") {
		t.Fatalf("%v", got)
	}
}

func TestAHostAlreadyAddedIsntAddedTwice(t *testing.T) {
	first, err := Add(nil, alias("devbox"))
	if err != nil {
		t.Fatal(err)
	}
	_, err = Add([]Host{first}, alias("devbox"))
	var exists ExistsError
	if !errors.As(err, &exists) || err.Error() != "devbox is already added" {
		t.Fatalf("%v", err)
	}
	if _, err := Add([]Host{first}, address("devbox")); err != nil {
		t.Fatalf("another way to reach it is another host: %v", err)
	}
}

func TestATargetThatCouldBecomeAnSshOptionIsRefused(t *testing.T) {
	const badAddr = "Enter an address like 10.0.4.12 or devbox.example.com"
	for _, c := range []struct {
		target Target
		want   string
	}{
		{alias(""), "Enter the host's address or name"},
		{address(""), "Enter the host's address or name"},
		{alias("-oProxyCommand=x"), badAddr},
		{address("-oProxyCommand=x"), badAddr},
		{alias("dev box"), badAddr},
		{address("10.0.4.12 "), badAddr},
		{Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 0}, "The port is a number, usually 22"},
		{Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 70000}, "The port is a number, usually 22"},
		{Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 22, User: "-x"}, "Enter a username without spaces"},
		{Target{Kind: KindAddress, Addr: "10.0.4.12", Port: 22, User: "a b"}, "Enter a username without spaces"},
		{Target{Kind: "teleport", Name: "x"}, "Enter the host's address or name"},
	} {
		err := Valid(c.target)
		var invalid InvalidError
		if !errors.As(err, &invalid) || err.Error() != c.want {
			t.Errorf("%+v: %v, want %q", c.target, err, c.want)
		}
		if _, err := Add(nil, c.target); err == nil {
			t.Errorf("%+v was added", c.target)
		}
	}
	for _, ok := range []Target{alias("devbox"), {Kind: KindAddress, Addr: "10.0.4.12", Port: 2222, User: "dev", KeyFile: "~/.ssh/k"}} {
		if err := Valid(ok); err != nil {
			t.Errorf("%+v: %v", ok, err)
		}
	}
}

func TestAHostsSessionIDCarriesTheHostIDAndUntagsBack(t *testing.T) {
	tagged := Tag("devbox", "7f3a")
	host, theirs := Untag(tagged)
	if tagged != "devbox/7f3a" || host != "devbox" || theirs != "7f3a" || !Tagged(tagged) {
		t.Fatalf("%q %q %q", tagged, host, theirs)
	}
	host, theirs = Untag("0b9c-11")
	if host != "" || theirs != "0b9c-11" || Tagged("0b9c-11") {
		t.Fatalf("%q %q", host, theirs)
	}
}
