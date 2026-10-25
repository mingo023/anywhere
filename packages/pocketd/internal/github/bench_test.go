package github

import (
	"encoding/json"
	"fmt"
	"strings"
	"testing"
)

// bigView is a PR whose 100 checks each ran three times, as reruns leave them in the rollup.
func bigView() string {
	var rollup []string
	for run := range 3 {
		for i := range 100 {
			rollup = append(rollup, fmt.Sprintf(`{"__typename":"CheckRun","name":"job-%d","workflowName":"CI","status":"COMPLETED","conclusion":"SUCCESS","startedAt":"2026-10-14T14:4%d:04Z","detailsUrl":"https://github.com/acme/app/actions/runs/%d/job/%d"}`, i, run, run, i))
		}
	}
	return `{"isDraft":false,"number":7,"state":"OPEN","title":"t","url":"u","mergeable":"MERGEABLE","statusCheckRollup":[` + strings.Join(rollup, ",") + `]}`
}

// bigThreads is GitHub's most: 100 threads of 50 comments, each with a long hunk.
func bigThreads() string {
	hunk := "@@ -1,40 +1,40 @@\n" + strings.Repeat(" context line of code here\n", 40)
	var threads []string
	for i := range 100 {
		var comments []string
		for j := range 50 {
			c, _ := json.Marshal(map[string]any{"author": map[string]string{"login": fmt.Sprintf("u%d", j%3)}, "body": strings.Repeat("A review comment. ", 20), "createdAt": "2026-10-02T18:37:02Z", "diffHunk": hunk})
			comments = append(comments, string(c))
		}
		threads = append(threads, fmt.Sprintf(`{"id":"PRRT_%d","isResolved":false,"isOutdated":false,"path":"src/f%d.rs","line":%d,"diffSide":"RIGHT","comments":{"nodes":[%s]}}`, i, i%10, i, strings.Join(comments, ",")))
	}
	return `{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[` + strings.Join(threads, ",") + `]}}}}}`
}

func BenchmarkParseAPRWith300CheckRuns(b *testing.B) {
	out := bigView()
	b.SetBytes(int64(len(out)))
	for b.Loop() {
		if pr, _ := viewResult(out, nil); len(pr.Runs) != 100 {
			b.Fatal(len(pr.Runs))
		}
	}
}

func BenchmarkParse100ThreadsOf50Comments(b *testing.B) {
	out := bigThreads()
	b.SetBytes(int64(len(out)))
	for b.Loop() {
		if threads, ok := threadsResult(out); !ok || len(threads) != 100 {
			b.Fatal(ok)
		}
	}
	threads, _ := threadsResult(out)
	raw, _ := json.Marshal(threads)
	b.ReportMetric(float64(len(raw)), "wire-bytes")
}
