package daemon

import (
	"encoding/json"
	"strings"

	"pocketd/internal/proto"
)

const (
	optionAlways = "always"
	optionAuto   = "auto"
)

var switchToAuto = json.RawMessage(`{"type":"setMode","mode":"auto","destination":"session"}`)

// options offers the choices Claude's own dialog shows between Yes and No.
func (in hookInput) options() []proto.PermissionOption {
	var opts []proto.PermissionOption
	if label := suggestionLabel(in.PermissionSuggestions); label != "" {
		opts = append(opts, proto.PermissionOption{ID: optionAlways, Label: label})
	}
	if in.PermissionMode != "auto" {
		opts = append(opts, proto.PermissionOption{ID: optionAuto, Label: "Yes, and switch to auto mode"})
	}
	return opts
}

func (in hookInput) updates(option string) []json.RawMessage {
	switch option {
	case optionAlways:
		return in.PermissionSuggestions
	case optionAuto:
		return []json.RawMessage{switchToAuto}
	}
	return nil
}

type suggestion struct {
	Type  string `json:"type"`
	Rules []struct {
		ToolName    string `json:"toolName"`
		RuleContent string `json:"ruleContent"`
	} `json:"rules"`
	Directories []string `json:"directories"`
	Mode        string   `json:"mode"`
}

// suggestionLabel phrases all suggestions as one choice, as Claude's dialog does.
func suggestionLabel(raw []json.RawMessage) string {
	var parts []string
	for _, r := range raw {
		var s suggestion
		json.Unmarshal(r, &s)
		switch s.Type {
		case "addRules":
			var rules []string
			for _, rule := range s.Rules {
				if rule.RuleContent == "" {
					rules = append(rules, rule.ToolName)
				} else {
					rules = append(rules, rule.ToolName+"("+rule.RuleContent+")")
				}
			}
			parts = append(parts, "don't ask again for "+strings.Join(rules, ", "))
		case "addDirectories":
			parts = append(parts, "always allow access to "+strings.Join(s.Directories, ", "))
		case "setMode":
			if s.Mode == "acceptEdits" {
				parts = append(parts, "allow all edits during this session")
			} else {
				parts = append(parts, "switch to "+s.Mode+" mode")
			}
		}
	}
	if len(parts) == 0 {
		return ""
	}
	return "Yes, and " + strings.Join(parts, " and ")
}
