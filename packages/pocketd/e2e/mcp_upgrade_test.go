//go:build unix

package e2e

import (
	"bytes"
	"errors"
	"net/http"
	"sync"
	"syscall"
	"testing"
)

func TestAToolCallThatConnectsDuringAnUpgradeIsAnsweredNotRefused(t *testing.T) {
	h, _, term, _ := chatClaude(t)
	url, auth := anywhereOf(t, h, term)

	var (
		wg      sync.WaitGroup
		stop    = make(chan struct{})
		mu      sync.Mutex
		answers int
		refused int
		cut     []error
	)
	wg.Add(1)
	go func() {
		defer wg.Done()
		for {
			select {
			case <-stop:
				return
			default:
			}
			req, _ := http.NewRequest(http.MethodPost, url, bytes.NewBufferString(`{"jsonrpc":"2.0","id":1,"method":"tools/list"}`))
			req.Header.Set("Content-Type", "application/json")
			req.Header.Set("Authorization", auth)
			resp, err := http.DefaultClient.Do(req)
			mu.Lock()
			switch {
			case err == nil:
				resp.Body.Close()
				answers++
			case errors.Is(err, syscall.ECONNREFUSED):
				refused++
			default:
				cut = append(cut, err)
			}
			mu.Unlock()
		}
	}()

	h.Upgrade()
	close(stop)
	wg.Wait()
	if got := toolsListStatus(t, url, auth); got != http.StatusOK {
		t.Fatalf("the upgraded pocketd answered tools/list with %d", got)
	}

	t.Logf("%d answered, %d refused, %d cut mid-call: %v", answers, refused, len(cut), cut)
	if refused > 0 {
		t.Fatalf("%d of %d tool calls were refused while pocketd upgraded", refused, refused+answers)
	}
}
