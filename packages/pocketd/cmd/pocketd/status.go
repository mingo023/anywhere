package main

import (
	"cmp"
	"encoding/json"
	"errors"
	"fmt"

	"pocketd/internal/ops"
)

// status exits 3 when no pocketd answers on sock.
func status(sock string, asJSON bool) (int, error) {
	c, err := ops.Dial(sock)
	if err != nil {
		fmt.Println("pocketd is not running")
		return 3, nil
	}
	defer c.Close()
	if err := c.Send(ops.Msg{Op: "status"}); err != nil {
		return 1, err
	}
	m, err := c.Recv()
	if err != nil {
		return 1, err
	}
	if m.Status == nil {
		return 1, errors.New(cmp.Or(m.Error, "no status in reply"))
	}
	if asJSON {
		out, _ := json.MarshalIndent(m.Status, "", "  ")
		fmt.Println(string(out))
		return 0, nil
	}
	fmt.Print(m.Status.Text())
	return 0, nil
}
