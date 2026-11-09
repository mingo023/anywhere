package teamrules

import "testing"

func TestStripLeavesOnlyWhatFollowsTheRules(t *testing.T) {
	for text, want := range map[string]string{
		"<anywhere_team>\nPlan it.\n</anywhere_team>\n\nAdd truncate\nand test it": "Add truncate\nand test it",
		"Add truncate": "Add truncate",
		"Tell me about <anywhere_team>\nx\n</anywhere_team>\n\ny": "Tell me about <anywhere_team>\nx\n</anywhere_team>\n\ny",
		"<anywhere_team>\nnever closed":                           "<anywhere_team>\nnever closed",
	} {
		if got := Strip(text); got != want {
			t.Errorf("%q: got %q, want %q", text, got, want)
		}
	}
}
