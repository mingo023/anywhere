package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strconv"
	"strings"
	"time"

	"pocketd/internal/config"
	"pocketd/internal/events"
)

func stats(args []string) (int, error) {
	fs := flag.NewFlagSet("stats", flag.ContinueOnError)
	since := fs.String("since", "7d", "window to report, like 7d or 12h")
	asJSON := fs.Bool("json", false, "print JSON")
	if fs.Parse(args) != nil || fs.NArg() > 0 {
		return 2, nil
	}
	window, err := parseWindow(*since)
	if err != nil {
		return 1, err
	}
	var files []io.Reader
	for _, name := range []string{"events.jsonl.2", "events.jsonl.1", "events.jsonl"} {
		f, err := os.Open(filepath.Join(config.Home(), name))
		if err != nil {
			continue
		}
		defer f.Close()
		files = append(files, f)
	}
	rep := events.Stats(io.MultiReader(files...), time.Now().Add(-window))
	if *asJSON {
		out, _ := json.MarshalIndent(rep, "", "  ")
		fmt.Println(string(out))
		return 0, nil
	}
	fmt.Print(rep.Text())
	return 0, nil
}

// parseWindow reads Go durations plus whole days ("7d").
func parseWindow(s string) (time.Duration, error) {
	if days, ok := strings.CutSuffix(s, "d"); ok {
		n, err := strconv.Atoi(days)
		if err != nil || n <= 0 {
			return 0, fmt.Errorf("--since %s: want days like 7d, or a duration like 12h", s)
		}
		return time.Duration(n) * 24 * time.Hour, nil
	}
	d, err := time.ParseDuration(s)
	if err != nil || d <= 0 {
		return 0, fmt.Errorf("--since %s: want days like 7d, or a duration like 12h", s)
	}
	return d, nil
}
