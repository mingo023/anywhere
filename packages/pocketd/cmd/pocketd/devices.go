package main

import (
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"text/tabwriter"
	"time"

	"pocketd/internal/devices"
	"pocketd/internal/ops"
)

func devicesCmd(sock string, args []string, w io.Writer) error {
	var req ops.Msg
	switch {
	case len(args) == 0, len(args) == 1 && args[0] == "--json":
		req = ops.Msg{Op: "devices"}
	case len(args) == 3 && args[0] == "rename":
		req = ops.Msg{Op: "devices.rename", ID: args[1], Text: args[2]}
	case len(args) == 2 && args[0] == "revoke":
		req = ops.Msg{Op: "devices.revoke", ID: args[1]}
	default:
		return errors.New(usage)
	}
	m, err := call(sock, req)
	if err != nil {
		return err
	}
	switch {
	case req.Op == "devices.rename":
		fmt.Fprintf(w, "Renamed to %s.\n", m.Text)
	case req.Op == "devices.revoke":
		fmt.Fprintf(w, "Removed %s. It was disconnected.\n", m.Text)
	case len(args) == 1:
		enc := json.NewEncoder(w)
		enc.SetIndent("", "  ")
		return enc.Encode(m.Devices)
	default:
		printDevices(w, m.Devices, time.Now())
	}
	return nil
}

func call(sock string, req ops.Msg) (ops.Msg, error) {
	c, err := ops.Dial(sock)
	if err != nil {
		return ops.Msg{}, err
	}
	defer c.Close()
	if err := c.Send(req); err != nil {
		return ops.Msg{}, err
	}
	m, err := c.Recv()
	if err == nil && m.Ev == "error" {
		err = errors.New(m.Error)
	}
	return m, err
}

func printDevices(w io.Writer, list []devices.Device, now time.Time) {
	tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
	fmt.Fprintln(tw, "ID\tNAME\tPLATFORM\tLAST SEEN")
	for _, d := range list {
		fmt.Fprintf(tw, "%s\t%s\t%s\t%s\n", d.ID[:min(8, len(d.ID))], d.Name, cmp.Or(d.Platform, "-"), ago(d.LastSeenAt, now))
	}
	tw.Flush()
}

func ago(ms int64, now time.Time) string {
	if ms == 0 {
		return "never"
	}
	d := now.Sub(time.UnixMilli(ms))
	switch {
	case d < time.Minute:
		return "now"
	case d < time.Hour:
		return fmt.Sprintf("%d min ago", int(d.Minutes()))
	case d < 24*time.Hour:
		return fmt.Sprintf("%d h ago", int(d.Hours()))
	}
	return fmt.Sprintf("%d days ago", int(d.Hours()/24))
}
