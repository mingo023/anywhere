package statedb

import (
	"bytes"
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"log"
	"maps"
	"os"
	"path/filepath"
	"slices"
	"strconv"
	"time"

	"pocketd/internal/automation"
	"pocketd/internal/board"
	"pocketd/internal/devices"
	"pocketd/internal/hosts"
	"pocketd/internal/names"
	"pocketd/internal/proto"
	"pocketd/internal/push"
	"pocketd/internal/registry"
	"pocketd/internal/worktree"
)

// importHome copies the JSON files a pocketd from before pocketd.db kept in home into tx, with the imported_at
// that marks it done. It never writes, renames or deletes a file. A missing file imports as empty, and so does
// one that can't be parsed, which is logged; one that can't be read fails the import, to run again next start.
func importHome(tx *sql.Tx, home string) error {
	reg, err := registry.ReadLegacy(home)
	if err != nil {
		return fmt.Errorf("import registry: %w", err)
	}
	f, err := importRegistry(tx, reg)
	if err != nil {
		return fmt.Errorf("import registry: %w", err)
	}
	ids := map[string]string{}
	for _, p := range f.Projects {
		ids[p] = f.Repos[p].ID
	}
	for _, step := range []func(*sql.Tx, string) error{
		importHosts,
		func(tx *sql.Tx, home string) error { return importLocals(tx, home, ids) },
		importNames,
		func(tx *sql.Tx, home string) error { return importBoard(tx, home, f) },
		func(tx *sql.Tx, home string) error { return importAutomations(tx, home, f) },
		importDevices,
		importPush,
		importConfig,
		importHostID,
	} {
		if err := step(tx, home); err != nil {
			return fmt.Errorf("import: %w", err)
		}
	}
	return setMeta(tx, "imported_at", strconv.FormatInt(time.Now().UnixMilli(), 10))
}

// readJSON decodes path into v and reports whether it did. A missing file is false; so is one that can't be
// parsed, logged by its path.
func readJSON(path string, v any) (bool, error) {
	raw, err := os.ReadFile(path)
	if errors.Is(err, fs.ErrNotExist) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	if err := json.Unmarshal(raw, v); err != nil {
		log.Printf("import: %s unreadable, imported as empty: %v", path, err)
		return false, nil
	}
	return true, nil
}

// readVersioned is readJSON for a file whose version must be 1, as every store that kept one refused another.
func readVersioned(path string, version *int, v any) (bool, error) {
	ok, err := readJSON(path, v)
	if ok && *version != 1 {
		log.Printf("import: %s has version %d, imported as empty", path, *version)
		return false, err
	}
	return ok, err
}

func setMeta(tx *sql.Tx, key, value string) error {
	_, err := tx.Exec(`INSERT INTO meta (key, value) VALUES (?, ?)`, key, value)
	return err
}

func marshal(v any) string {
	raw, _ := json.Marshal(v)
	return string(raw)
}

// dropped logs a row the import left out and why.
func dropped(what, id, why string) {
	log.Printf("import: dropped %s %s: %s", what, id, why)
}

// importRegistry writes s, each project and worktree once, and is the registry the rest is placed in.
func importRegistry(tx *sql.Tx, s registry.State) (registry.File, error) {
	seenID, seenPath := map[string]bool{}, map[string]bool{}
	var kept []proto.RegistryProject
	for _, p := range s.Projects {
		if seenID[p.ID] || seenPath[p.Path] {
			dropped("project", p.Path, "listed twice")
			continue
		}
		seenID[p.ID], seenPath[p.Path] = true, true
		kept = append(kept, p)
	}
	s.Projects, s.Tracked, s.AgentWorktrees = kept, unique(s.Tracked), unique(s.AgentWorktrees)
	return s.File(), registry.Insert(tx, s)
}

func unique(paths []string) []string {
	var out []string
	for _, p := range paths {
		if !slices.Contains(out, p) {
			out = append(out, p)
		}
	}
	return out
}

func importHosts(tx *sql.Tx, home string) error {
	var f struct {
		Version int          `json:"version"`
		Hosts   []hosts.Host `json:"hosts"`
	}
	path := filepath.Join(home, "state", "hosts.json")
	if _, err := os.Stat(path); errors.Is(err, fs.ErrNotExist) {
		if f.Hosts, err = hosts.DesktopHosts(home); err != nil {
			log.Printf("import: hosts: %v", err)
		}
	} else if ok, err := readVersioned(path, &f.Version, &f); err != nil {
		return err
	} else if !ok {
		f.Hosts = nil
	}
	seen := map[string]bool{}
	for _, h := range f.Hosts {
		if seen[h.ID] {
			dropped("host", h.ID, "listed twice")
			continue
		}
		seen[h.ID] = true
		if _, err := tx.Exec(`INSERT INTO hosts (id, name, target, windows, position) VALUES (?, ?, ?, ?, ?)`, h.ID, h.Name, marshal(h.Target), h.Windows, len(seen)-1); err != nil {
			return err
		}
	}
	return nil
}

func importLocals(tx *sql.Tx, home string, projects map[string]string) error {
	var locals []struct {
		proto.Local
		Auto bool `json:"auto,omitempty"`
	}
	if ok, err := readJSON(filepath.Join(home, "state", "locals.json"), &locals); err != nil || !ok {
		return err
	}
	byPath := map[string]string{}
	for p, id := range projects {
		byPath[filepath.Clean(p)] = id
	}
	seen := map[string]bool{}
	for _, l := range locals {
		id, ok := byPath[filepath.Clean(l.Project)]
		switch {
		case !ok:
			dropped("local", l.ID, "its folder "+l.Project+" is no project")
			continue
		case seen[l.ID]:
			dropped("local", l.ID, "listed twice")
			continue
		}
		seen[l.ID] = true
		if _, err := tx.Exec(`INSERT INTO locals (id, project_id, name, auto, position) VALUES (?, ?, ?, ?, ?)`, l.ID, id, l.Name, l.Auto, len(seen)-1); err != nil {
			return err
		}
	}
	return nil
}

func importNames(tx *sql.Tx, home string) error {
	var entries map[string]names.Entry
	if ok, err := readJSON(filepath.Join(home, "state", "names.json"), &entries); err != nil || !ok {
		return err
	}
	for path, e := range entries {
		if _, err := tx.Exec(`INSERT INTO names (path, title, auto) VALUES (?, ?, ?)`, path, e.Title, e.Auto); err != nil {
			return err
		}
	}
	return nil
}

func importBoard(tx *sql.Tx, home string, reg registry.File) error {
	var f board.File
	ok, err := readVersioned(filepath.Join(home, "state", "board.json"), &f.Version, &f)
	if err != nil || !ok {
		return err
	}
	for i := range f.Tickets {
		if f.Tickets[i].Number == 0 {
			f.Tickets[i].Number = f.NextNumber()
		}
	}
	seen := map[string]bool{}
	for _, t := range f.Tickets {
		id, tree, ok := worktree.Belong(reg, t.Folder)
		switch {
		case !ok:
			dropped("ticket", t.ID, "its folder "+t.Folder+" is in no project")
			continue
		case seen[t.ID]:
			dropped("ticket", t.ID, "listed twice")
			continue
		}
		seen[t.ID] = true
		labels := t.Labels
		if labels == nil {
			labels = []string{}
		}
		if _, err := tx.Exec(`INSERT INTO tickets (id, project_id, tree, title, prompt, provider, base, state, sort_order, status, why, agent_id, terminal_id,
			worktree, created_at, changed_at, closed_at, closed_why, relaunch, number, priority, labels, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			t.ID, id, tree, t.Title, t.Prompt, t.Provider, t.Base, t.State, t.Order, t.Status, t.Why, t.AgentID, t.TerminalID,
			t.Worktree, t.CreatedAt, t.ChangedAt, t.ClosedAt, t.ClosedWhy, t.Relaunch, t.Number, t.Priority, marshal(labels), len(seen)-1); err != nil {
			return err
		}
	}
	if err := setMeta(tx, "board.limit", strconv.Itoa(f.Limit)); err != nil {
		return err
	}
	return setMeta(tx, "board.last_number", strconv.FormatInt(f.LastNumber, 10))
}

func importAutomations(tx *sql.Tx, home string, reg registry.File) error {
	var f automation.File
	ok, err := readVersioned(filepath.Join(home, "state", "automations.json"), &f.Version, &f)
	if err != nil || !ok {
		return err
	}
	kept := map[string]bool{}
	for _, a := range f.Automations {
		id, tree, ok := worktree.Belong(reg, a.Folder)
		switch {
		case !ok:
			dropped("automation", a.ID, "its folder "+a.Folder+" is in no project")
			continue
		case kept[a.ID]:
			dropped("automation", a.ID, "listed twice")
			continue
		}
		kept[a.ID] = true
		if _, err := tx.Exec(`INSERT INTO automations (id, project_id, tree, name, prompt, provider, schedule, enabled, next_run_at, access, new_worktree,
			base, model, effort, created_by, bound, webhook, webhook_secret, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			a.ID, id, tree, a.Name, a.Prompt, a.Provider, marshal(a.Schedule), a.Enabled, a.NextRunAt, a.Access, a.NewWorktree,
			a.Base, a.Model, a.Effort, a.CreatedBy, a.Bound, a.Webhook, a.WebhookSecret, len(kept)-1); err != nil {
			return err
		}
	}
	seen := map[string]bool{}
	for _, r := range f.Runs {
		switch {
		case !kept[r.AutomationID]:
			dropped("run", r.ID, "its automation "+r.AutomationID+" wasn't imported")
			continue
		case seen[r.ID]:
			dropped("run", r.ID, "listed twice")
			continue
		}
		seen[r.ID] = true
		if _, err := tx.Exec(`INSERT INTO runs (id, automation_id, status, trigger, why, summary, started_at, finished_at, agent_id, terminal_id,
			access, worktree, turn_after, turn_prompt, turn_entry, position) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			r.ID, r.AutomationID, r.Status, r.Trigger, r.Why, r.Summary, r.StartedAt, r.FinishedAt, r.AgentID, r.TerminalID,
			r.Access, r.Worktree, r.TurnAfter, r.TurnPrompt, r.TurnEntry, len(seen)-1); err != nil {
			return err
		}
	}
	return nil
}

func importDevices(tx *sql.Tx, home string) error {
	var f struct {
		Version    int              `json:"version"`
		Devices    []devices.Device `json:"devices"`
		GraceEnded bool             `json:"graceEnded,omitempty"`
	}
	ok, err := readVersioned(filepath.Join(home, "devices.json"), &f.Version, &f)
	if err != nil || !ok {
		return err
	}
	seen := map[string]bool{}
	for _, d := range f.Devices {
		if seen[d.ID] {
			dropped("device", d.ID, "listed twice")
			continue
		}
		seen[d.ID] = true
		scopes := d.Scopes
		if scopes == nil {
			scopes = []devices.Scope{}
		}
		if _, err := tx.Exec(`INSERT INTO devices (id, name, platform, scopes, token_hash, created_at, last_seen_at, last_addr, legacy, grace_ends_at, position)
			VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			d.ID, d.Name, d.Platform, marshal(scopes), d.TokenHash, d.CreatedAt, d.LastSeenAt, d.LastAddr, d.Legacy, d.GraceEndsAt, len(seen)-1); err != nil {
			return err
		}
	}
	return setMeta(tx, "devices.grace_ended", strconv.FormatBool(f.GraceEnded))
}

// nullJSON is v as JSON, or NULL for a nil pointer.
func nullJSON[T any](v *T) *string {
	if v == nil {
		return nil
	}
	s := marshal(v)
	return &s
}

func importPush(tx *sql.Tx, home string) error {
	var f struct {
		Records map[string]*push.Record `json:"records"`
		Sends   []push.Sent             `json:"sends"`
	}
	ok, err := readJSON(push.Path(home), &f)
	if err != nil || !ok {
		return err
	}
	for device, r := range f.Records {
		if r == nil {
			continue
		}
		if _, err := tx.Exec(`INSERT INTO push_records (device, key, env, alert, start, activity, prefs, frequent, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
			device, r.Key, r.Env, nullJSON(r.Alert), nullJSON(r.Start), nullJSON(r.Activity), marshal(r.Prefs), r.Frequent, r.UpdatedAt); err != nil {
			return err
		}
	}
	for i, s := range f.Sends {
		if _, err := tx.Exec(`INSERT INTO push_sends (position, at, device, kind, agent, priority, outcome) VALUES (?, ?, ?, ?, ?, ?, ?)`,
			i, s.At, s.Device, s.Kind, s.Agent, s.Priority, s.Outcome); err != nil {
			return err
		}
	}
	return nil
}

// importConfig keeps each key of config.json under the name `pocketd config set` takes: a top-level value under
// its key, each member of a top-level object under "key.member", and an empty object under its key as {}.
// Values are compact JSON. Keys pocketd doesn't know, and a token an older pocketd left, are kept. The scalar
// settings stay whole whatever they hold, so config.Load refuses one that isn't a scalar rather than defaulting it.
var scalarConfig = map[string]bool{"port": true, "listen": true, "ownerListen": true, "token": true}

func importConfig(tx *sql.Tx, home string) error {
	var top map[string]json.RawMessage
	if _, err := readJSON(filepath.Join(home, "config.json"), &top); err != nil {
		return err
	}
	set := func(key string, raw json.RawMessage) error {
		var b bytes.Buffer
		if err := json.Compact(&b, raw); err != nil {
			return err
		}
		res, err := tx.Exec(`INSERT INTO config (key, value) VALUES (?, ?) ON CONFLICT (key) DO NOTHING`, key, b.String())
		if err != nil {
			return err
		}
		if n, _ := res.RowsAffected(); n == 0 {
			dropped("config key", key, "it is there twice, once dotted")
		}
		return nil
	}
	// Sorted, so of "a.b" and "a": {"b"}, the one `pocketd config set` writes comes first and is kept.
	for _, key := range slices.Sorted(maps.Keys(top)) {
		raw := top[key]
		var inner map[string]json.RawMessage
		if scalarConfig[key] || json.Unmarshal(raw, &inner) != nil || len(inner) == 0 {
			if err := set(key, raw); err != nil {
				return err
			}
			continue
		}
		for member, v := range inner {
			if err := set(key+"."+member, v); err != nil {
				return err
			}
		}
	}
	return nil
}

func importHostID(tx *sql.Tx, home string) error {
	var f struct {
		ID string `json:"id"`
	}
	if ok, err := readJSON(filepath.Join(home, "state", "host.json"), &f); err != nil || !ok || f.ID == "" {
		return err
	}
	return setMeta(tx, "host_id", f.ID)
}
