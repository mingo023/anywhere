package proc

import (
	"encoding/binary"
	"slices"
	"testing"
)

func procargs2(argc uint32, s string) []byte {
	return append(binary.NativeEndian.AppendUint32(nil, argc), s...)
}

func TestParse(t *testing.T) {
	for _, tc := range []struct {
		name      string
		b         []byte
		argv, env []string
	}{
		{"normal", procargs2(2, "/bin/claude\x00\x00\x00claude\x00-p\x00A=1\x00B=2\x00\x00ptr_munge=\x00"), []string{"claude", "-p"}, []string{"A=1", "B=2"}},
		{"title rewritten", procargs2(3, "/bin/node\x00\x00\x00claude\x00\x00\x00\x00\x00\x00\x00\x00ptr_munge=\x00"), []string{"claude"}, nil},
		{"empty", nil, nil, nil},
		{"short", []byte{2, 0, 0}, nil, nil},
	} {
		if argv, env := parse(tc.b); !slices.Equal(argv, tc.argv) || !slices.Equal(env, tc.env) {
			t.Errorf("%s: parse = %q, %q; want %q, %q", tc.name, argv, env, tc.argv, tc.env)
		}
	}
}
