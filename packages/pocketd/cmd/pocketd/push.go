package main

import (
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"path/filepath"
	"strings"
	"text/tabwriter"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/push"
)

// pushStatus prints each phone's push registration and the last sends, read
// from push.json; it shows when handles expire, never their keys or secrets.
func pushStatus(home string, args []string, w io.Writer, now time.Time) error {
	if len(args) > 1 || len(args) == 1 && args[0] != "--json" {
		return errors.New(usage)
	}
	store, err := push.Open(push.Path(home))
	if err != nil {
		return err
	}
	names := map[string]string{}
	if devs, err := devices.Open(filepath.Join(home, "devices.json"), ""); err == nil {
		for _, d := range devs.List() {
			names[d.ID] = d.Name
		}
	}
	type phone struct {
		Device       string `json:"device"`
		Name         string `json:"name,omitempty"`
		Env          string `json:"env"`
		AlertUntil   int64  `json:"alertExpiresAt,omitempty"`
		StartUntil   int64  `json:"startExpiresAt,omitempty"`
		Activity     string `json:"activity,omitempty"`
		Prefs        string `json:"prefs"`
		Frequent     bool   `json:"frequent"`
		RegisteredAt int64  `json:"registeredAt"`
	}
	var phones []phone
	for _, r := range store.Records() {
		p := phone{Device: r.Device, Name: names[r.Device], Env: r.Env, Prefs: prefs(r), Frequent: r.Frequent, RegisteredAt: r.UpdatedAt}
		if r.Alert != nil {
			p.AlertUntil = r.Alert.ExpiresAt
		}
		if r.Start != nil {
			p.StartUntil = r.Start.ExpiresAt
		}
		if r.Activity != nil {
			p.Activity = r.Activity.ID
			if r.Activity.Handle == nil {
				p.Activity += " (no token yet)"
			}
		}
		phones = append(phones, p)
	}
	sends := store.Sends()
	if len(args) == 1 {
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")
		return enc.Encode(map[string]any{"phones": phones, "sends": sends})
	}
	if len(phones) == 0 {
		fmt.Fprintln(w, "No phone is registered for pushes.")
	}
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	if len(phones) > 0 {
		fmt.Fprintln(tw, "DEVICE\tNAME\tALERTS\tLIVE ACTIVITY\tWANTS")
	}
	for _, p := range phones {
		alerts := "gone"
		if p.AlertUntil != 0 {
			alerts = "until " + time.Unix(p.AlertUntil, 0).Format(time.DateOnly)
		}
		activity := p.Activity
		if activity == "" {
			activity = "-"
			if p.StartUntil != 0 {
				activity = "starts on Needs you"
			}
		}
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\t%s\n", p.Device[:min(8, len(p.Device))], p.Name, alerts, activity, p.Prefs)
	}
	tw.Flush()
	if len(sends) == 0 {
		return nil
	}
	fmt.Fprintln(w, "\nLast sends:")
	tw = tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	for i := len(sends) - 1; i >= 0; i-- {
		s := sends[i]
		fmt.Fprintf(tw, "%s\t%s\t%s\tp%d\t%s\n", ago(s.At, now), s.Device[:min(8, len(s.Device))], s.Kind, s.Priority, s.Outcome)
	}
	return tw.Flush()
}

func prefs(r push.Record) string {
	var on []string
	for _, p := range []struct {
		on   bool
		name string
	}{{r.Prefs.NeedsYou, "needs you"}, {r.Prefs.Done, "done"}, {r.Prefs.Failed, "failed"}, {r.Prefs.Automations, "automations"}} {
		if p.on {
			on = append(on, p.name)
		}
	}
	if len(on) == 0 {
		return "none"
	}
	return strings.Join(on, ", ")
}
