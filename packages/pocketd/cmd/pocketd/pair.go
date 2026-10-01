package main

import (
	"errors"
	"fmt"
	"io"
	"net/url"
	"time"

	"pocketd/internal/ops"
	"pocketd/internal/pairing"
)

// pairCmd shows a pairing code and waits for a phone to use it. tty is
// whether stdin is a terminal; the server's peer check is the real gate.
func pairCmd(sock string, args []string, tty bool, w, errw io.Writer) (int, error) {
	if !tty {
		fmt.Fprintln(errw, "Run pocketd pair in a terminal.")
		return 2, nil
	}
	req := ops.Msg{Op: "pair.begin"}
	switch {
	case len(args) == 2 && args[0] == "--host":
		req.Text = args[1]
	case len(args) != 0:
		return 1, errors.New(usage)
	}
	c, err := ops.Dial(sock)
	if err != nil {
		return 1, err
	}
	defer c.Close()
	if err := c.Send(req); err != nil {
		return 1, err
	}
	m, err := c.Recv()
	if err != nil {
		return 1, err
	}
	if m.Ev == "error" {
		return 1, errors.New(m.Error)
	}
	art, err := qrText(m.Pair.URL)
	if err != nil {
		return 1, err
	}
	fmt.Fprint(w, art)
	fmt.Fprint(w, offerLine(*m.Pair))
	m, err = c.Recv()
	if err != nil {
		return 1, errors.New("pocketd stopped. Run pocketd pair again.")
	}
	line, code := pairEnd(m)
	fmt.Fprintln(w, line)
	return code, nil
}

func offerLine(o pairing.Offer) string {
	host := ""
	if u, err := url.Parse(o.URL); err == nil {
		host = u.Query().Get("h")
	}
	return fmt.Sprintf("Scan with the iPhone Camera app, or enter  %s  %s  Expires at %s.\n", host, o.Code, time.UnixMilli(o.ExpiresAt).Format("15:04"))
}

func pairEnd(m ops.Msg) (string, int) {
	switch {
	case m.Ev == "pair.ok":
		return "Paired " + m.Text + ".", 0
	case m.ErrorCode == "pair_locked":
		return "Too many wrong codes. Run pocketd pair again in a minute.", 1
	case m.ErrorCode == "pair_failed":
		return "Pairing failed. Run pocketd pair again.", 1
	}
	return "Code expired. Run pocketd pair again.", 1
}
