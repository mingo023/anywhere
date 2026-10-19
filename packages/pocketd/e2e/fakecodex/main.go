// Command fakecodex stands in for the codex CLI in e2e tests: it prints its
// arguments and echoes input. As `codex app-server` it speaks JSON-RPC, as
// for Pocket's chat; see appserver.go.
package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "app-server" {
		appServerMain()
		return
	}
	fmt.Printf("fake codex ready %s\r\n", strings.Join(os.Args[1:], " "))
	in := bufio.NewScanner(os.Stdin)
	for in.Scan() {
		fmt.Printf("typed: %s\r\n", in.Text())
	}
}
