package orchhost

import (
	"encoding/json"
	"errors"

	"pocketd/internal/agent"
	"pocketd/internal/launch"
	"pocketd/internal/orch"
)

// sendNow has the agent send the prompts it holds before its current step ends.
func sendNow(a *agent.Agent) error {
	n, ok := a.Driver().(agent.SendNower)
	if !ok {
		return errors.New("This agent takes a message only after its current step")
	}
	return n.SendNow()
}

// create runs a client's agent.create. Its result is the whole launch.Result,
// failure and all, for a retry to read back; the requester watches its lane.
func (h *Host) create(e orch.Effect) (map[string]any, error) {
	var p orch.StartPayload
	json.Unmarshal(e.Payload, &p)
	r := h.Launcher.Create(launch.Who{Owner: p.Owner, Key: p.Key}, e.Lane, p.Spec, func(string, string) {}, func(launch.Creating) {})
	var res map[string]any
	b, _ := json.Marshal(r)
	json.Unmarshal(b, &res)
	if r.Err != nil {
		return res, errors.New(r.Err.Message)
	}
	return res, nil
}
