package launch

import (
	"errors"

	"pocketd/internal/agent"
)

// Switch is the user's agent.settings on a running agent: the model and
// effort, then the model's options, then the access; the first refusal stops it.
func Switch(a *agent.Agent, model, effort string, options map[string]string, access string) error {
	if model != "" || effort != "" {
		if err := ValidSwitch(a.Provider(), model, effort); err != nil {
			return err
		}
		if err := a.Driver().Settings(model, effort); err != nil {
			return err
		}
	}
	if len(options) > 0 {
		if err := ValidOptions(options); err != nil {
			return err
		}
		o, ok := a.Driver().(agent.Optioner)
		if !ok {
			return errors.New("This agent takes its model's settings only as it starts")
		}
		if err := o.SetOptions(options); err != nil {
			return err
		}
	}
	if access != "" {
		return a.Driver().Access(access)
	}
	return nil
}
