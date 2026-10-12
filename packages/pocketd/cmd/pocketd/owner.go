package main

import (
	"cmp"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"net"
	"strconv"
	"text/tabwriter"
	"time"

	"pocketd/internal/config"
	"pocketd/internal/owners"
)

type ownerToken struct {
	Token string `json:"token"`
	ID    string `json:"id"`
	Port  int    `json:"port"`
}

// ownerPair mints an owner token for a desktop that reaches this host over
// ownerListen. Writing home is the gate: no pid is owner on Windows.
func ownerPair(home string, args []string, w io.Writer) error {
	name := ""
	switch {
	case len(args) == 3 && args[0] == "--json" && args[1] == "--name":
		name = args[2]
	case len(args) != 1 || args[0] != "--json":
		return errors.New(usage)
	}
	addr, err := config.OwnerListen()
	if err != nil {
		return err
	}
	if addr == "off" {
		return errors.New("ownerListen is off; set it in config.json")
	}
	_, p, _ := net.SplitHostPort(addr)
	port, _ := strconv.Atoi(p)
	o, token, err := owners.Store{Path: owners.Path(home)}.Mint(name)
	if err != nil {
		return err
	}
	return json.NewEncoder(w).Encode(ownerToken{token, o.ID, port})
}

func ownerCmd(home string, args []string, w io.Writer) error {
	s := owners.Store{Path: owners.Path(home)}
	switch {
	case len(args) == 1 && args[0] == "list":
		list, err := s.List()
		if err != nil {
			return err
		}
		tw := tabwriter.NewWriter(w, 0, 0, 2, ' ', 0)
		fmt.Fprintln(tw, "ID\tNAME\tCREATED")
		for _, o := range list {
			fmt.Fprintf(tw, "%s\t%s\t%s\n", o.ID[:8], cmp.Or(o.Name, "-"), time.UnixMilli(o.CreatedAt).Format(time.DateOnly))
		}
		return tw.Flush()
	case len(args) == 2 && args[0] == "revoke":
		o, err := s.Revoke(args[1])
		if err != nil {
			return err
		}
		fmt.Fprintf(w, "Revoked %s. Its desktop is disconnected within %s.\n", o.Name, owners.WatchEvery)
		return nil
	}
	return errors.New(usage)
}
