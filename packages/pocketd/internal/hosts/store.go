package hosts

import (
	"database/sql"
	"encoding/json"
	"errors"
	"fmt"
	"io/fs"
	"os"
	"path/filepath"
	"slices"
)

// The db may be read-only.
func Load(db *sql.DB) ([]Host, error) {
	rows, err := db.Query(`SELECT id, name, target, windows FROM hosts ORDER BY position`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	var hosts []Host
	for rows.Next() {
		var h Host
		var target string
		if err := rows.Scan(&h.ID, &h.Name, &target, &h.Windows); err != nil {
			return nil, err
		}
		if err := json.Unmarshal([]byte(target), &h.Target); err != nil {
			return nil, fmt.Errorf("hosts: host %s: %w", h.ID, err)
		}
		hosts = append(hosts, h)
	}
	return hosts, rows.Err()
}

// One transaction.
func Save(db *sql.DB, prev, next []Host) error {
	tx, err := db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()
	for _, h := range prev {
		if !slices.ContainsFunc(next, func(n Host) bool { return n.ID == h.ID }) {
			if _, err := tx.Exec(`DELETE FROM hosts WHERE id = ?`, h.ID); err != nil {
				return err
			}
		}
	}
	for i, h := range next {
		if i < len(prev) && prev[i] == h {
			continue
		}
		target, _ := json.Marshal(h.Target)
		if _, err := tx.Exec(`INSERT INTO hosts (id, name, target, windows, position) VALUES (?, ?, ?, ?, ?)
			ON CONFLICT (id) DO UPDATE SET name = excluded.name, target = excluded.target, windows = excluded.windows, position = excluded.position`,
			h.ID, h.Name, string(target), h.Windows, i); err != nil {
			return err
		}
	}
	return tx.Commit()
}

// desktopHost is a host as the desktop saved it in desktop.json, which spells a target's key file key_file.
type desktopHost struct {
	ID     string `json:"id"`
	Name   string `json:"name"`
	Target struct {
		Kind    string `json:"kind"`
		Name    string `json:"name"`
		Addr    string `json:"addr"`
		Port    int    `json:"port"`
		User    string `json:"user"`
		KeyFile string `json:"key_file"`
	} `json:"target"`
	Windows bool `json:"windows"`
}

// DesktopHosts is the hosts the desktop kept in home/desktop.json; nil when that file is missing.
func DesktopHosts(home string) ([]Host, error) {
	raw, err := os.ReadFile(filepath.Join(home, "desktop.json"))
	if errors.Is(err, fs.ErrNotExist) {
		return nil, nil
	}
	var f struct {
		Hosts []desktopHost `json:"hosts"`
	}
	if err == nil {
		err = json.Unmarshal(raw, &f)
	}
	if err != nil {
		return nil, fmt.Errorf("desktop.json: %w", err)
	}
	hosts := make([]Host, len(f.Hosts))
	for i, h := range f.Hosts {
		hosts[i] = Host{ID: h.ID, Name: h.Name, Target: Target(h.Target), Windows: h.Windows}
	}
	return hosts, nil
}
