// Command fakecodex stands in for the codex CLI in e2e tests: it accepts
// `app-server daemon start`, otherwise prints its arguments and echoes input.
package main

import (
	"bufio"
	"fmt"
	"os"
	"strings"
)

func main() {
	if len(os.Args) > 1 && os.Args[1] == "app-server" {
		return
	}
	fmt.Printf("fake codex ready %s\r\n", strings.Join(os.Args[1:], " "))
	in := bufio.NewScanner(os.Stdin)
	for in.Scan() {
		fmt.Printf("typed: %s\r\n", in.Text())
	}
}
