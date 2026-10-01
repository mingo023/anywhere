package proto

import (
	"slices"
	"testing"
)

func TestTheServerOffersTheRegistryCap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapRegistry}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapRegistry}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}

func TestTheServerOffersTheSummaryV2Cap(t *testing.T) {
	_, caps, code := Negotiate(Range{Min: 3, Max: 3}, []string{CapSummaryV2}, ServerCaps)
	if code != "" || !slices.Equal(caps, []string{CapSummaryV2}) {
		t.Fatalf("caps %v, code %q", caps, code)
	}
}
