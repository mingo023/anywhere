package main

import (
	"encoding/json"
	"flag"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"slices"
	"text/tabwriter"

	"pocketd/internal/proto"
	"pocketd/internal/registry"
	"pocketd/internal/worktree"
)

const worktreeUsage = "usage: pocketd worktree list [--json] [<project>] | create [--project <path>] [--base <branch>] [--no-copy] <name> | remove <path>"

func worktreeMain(args []string, stdout, stderr io.Writer) int {
	if len(args) == 0 {
		fmt.Fprintln(stderr, worktreeUsage)
		return 2
	}
	f := registry.New(registry.Path()).Load()
	flags := flag.NewFlagSet("pocketd worktree "+args[0], flag.ContinueOnError)
	flags.SetOutput(stderr)
	var err error
	switch args[0] {
	case "list":
		asJSON := flags.Bool("json", false, "print []proto.Project")
		if flags.Parse(args[1:]) != nil || flags.NArg() > 1 {
			return 2
		}
		err = list(f, flags.Arg(0), *asJSON, stdout)
	case "create":
		project := flags.String("project", "", "registered project path")
		base := flags.String("base", "", "branch to start from")
		noCopy := flags.Bool("no-copy", false, "skip the copy set")
		if flags.Parse(args[1:]) != nil || flags.NArg() != 1 {
			return 2
		}
		err = create(f, *project, *base, flags.Arg(0), *noCopy, stdout, stderr)
	case "remove":
		if flags.Parse(args[1:]) != nil || flags.NArg() != 1 {
			return 2
		}
		err = remove(f, flags.Arg(0))
	default:
		fmt.Fprintln(stderr, worktreeUsage)
		return 2
	}
	if err != nil {
		fmt.Fprintln(stderr, "pocketd worktree:", err)
		return 1
	}
	return 0
}

// projectFor is arg as a registered Project, or the one holding the cwd when
// arg is empty.
func projectFor(f registry.File, arg string) (string, error) {
	dir, _ := os.Getwd()
	if arg != "" {
		dir, _ = filepath.Abs(arg)
		if !slices.Contains(f.Projects, dir) {
			return "", fmt.Errorf("%w: %s", worktree.ErrUnknownProject, dir)
		}
	}
	p, ok := worktree.Containing(f, dir)
	if !ok {
		return "", fmt.Errorf("%w: %s", worktree.ErrUnknownProject, dir)
	}
	return p, nil
}

func list(f registry.File, arg string, asJSON bool, stdout io.Writer) error {
	p, err := projectFor(f, arg)
	if err != nil {
		return err
	}
	projects := worktree.Projects(registry.File{Projects: []string{p}, Repos: f.Repos})
	if asJSON {
		enc := json.NewEncoder(stdout)
		enc.SetIndent("", "  ")
		return enc.Encode(projects)
	}
	tw := tabwriter.NewWriter(stdout, 0, 4, 2, ' ', 0)
	for _, pr := range projects {
		for _, w := range pr.Worktrees {
			fmt.Fprintf(tw, "%s\t%s\t%s\t%s\n", pr.Name, w.Name, w.Path, checkout(w))
		}
	}
	return tw.Flush()
}

func checkout(w proto.Worktree) string {
	switch {
	case w.IsMain:
		return "(main)"
	case w.Branch == "":
		return "detached"
	}
	return "on " + w.Branch
}

func create(f registry.File, project, base, name string, noCopy bool, stdout, stderr io.Writer) error {
	p, err := projectFor(f, project)
	if err != nil {
		return err
	}
	var c worktree.Created
	if noCopy {
		c, err = worktree.Add(f, p, name, base)
	} else {
		c, err = worktree.Create(f, p, name, base)
	}
	if err != nil {
		return err
	}
	for _, s := range c.Skipped {
		fmt.Fprintln(stderr, "skipped", s)
	}
	fmt.Fprintln(stdout, c.Path)
	return nil
}

func remove(f registry.File, path string) error {
	abs, _ := filepath.Abs(path)
	p, ok := worktree.Containing(f, abs)
	if !ok {
		return fmt.Errorf("%w: %s", worktree.ErrUnknownWorktree, abs)
	}
	return worktree.Remove(f, p, abs)
}
