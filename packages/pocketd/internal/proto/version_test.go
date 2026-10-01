package proto

import (
	"reflect"
	"testing"
)

func TestNegotiateNamesTheOlderSide(t *testing.T) {
	for _, c := range []struct {
		client  Range
		version int
		code    string
	}{
		{Range{3, 3}, 3, ""},
		{Range{2, 5}, 3, ""},
		{Range{1, 2}, 0, "client_too_old"},
		{Range{4, 6}, 0, "server_too_old"},
	} {
		version, _, code := Negotiate(c.client, nil, nil)
		if version != c.version || code != c.code {
			t.Errorf("%v: got %d %q, want %d %q", c.client, version, code, c.version, c.code)
		}
	}
}

func TestNegotiateKeepsOnlyCapsBothSidesHave(t *testing.T) {
	_, caps, _ := Negotiate(Range{3, 3}, []string{"x.v1", "pair.v1", "pair.v1"}, []string{"pair.v1", "host.v1"})
	if !reflect.DeepEqual(caps, []string{"pair.v1"}) {
		t.Fatalf("%v", caps)
	}
	if _, caps, _ = Negotiate(Range{3, 3}, nil, []string{"pair.v1"}); caps == nil || len(caps) != 0 {
		t.Fatalf("want empty non-nil caps, got %#v", caps)
	}
}

func TestAHelloWithoutARangeMeansItsVersion(t *testing.T) {
	m, err := DecodeClient([]byte(`{"type":"hello","id":"h","token":"t","clientId":"c","protocolVersion":2}`))
	if err != nil || m.Versions() != (Range{2, 2}) {
		t.Fatalf("%v %v", m.Versions(), err)
	}
	m, err = DecodeClient([]byte(`{"type":"hello","id":"h","token":"t","clientId":"c","protocolVersion":4,"protocol":{"min":3,"max":4}}`))
	if err != nil || m.Versions() != (Range{3, 4}) {
		t.Fatalf("%v %v", m.Versions(), err)
	}
}
