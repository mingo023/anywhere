package orchhost

import (
	"errors"

	"pocketd/internal/agent"
)

// sendNow has the agent send the prompts it holds before its current step ends.
func sendNow(a *agent.Agent) error {
	n, ok := a.Driver().(agent.SendNower)
	if !ok {
		return errors.New("This agent takes a message only after its current step")
	}
	return n.SendNow()
}
