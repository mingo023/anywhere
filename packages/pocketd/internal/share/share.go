// Package share is the MCP server pocketd's Claude plugin runs. Its one
// tool, share_file, keeps a copy of a file for the chat to show the user.
package share

import (
	"crypto/sha256"
	"encoding/hex"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"io/fs"
	"os"
	"path/filepath"

	"pocketd/internal/atomicfile"
)

// Max is the largest file a share keeps: the most fs.read sends the phone
// (fsview.ImageMax).
const Max = 20 << 20

// Keep copies the file at path into dir as <hash>-<name> and returns where.
// A relative path is from the folder the server runs in, the agent's. The
// same bytes shared again land on the copy kept before, so a card keeps
// showing what it showed when the file changes after.
func Keep(dir, path string) (string, error) {
	if path == "" {
		return "", errors.New("Name the file to share in path")
	}
	full, err := filepath.Abs(path)
	if err != nil {
		return "", err
	}
	info, err := os.Stat(full)
	switch {
	case errors.Is(err, fs.ErrNotExist):
		return "", fmt.Errorf("There's no file at %s", path)
	case err != nil:
		return "", err
	case info.IsDir():
		return "", fmt.Errorf("%s is a folder; share a file in it", path)
	case !info.Mode().IsRegular():
		return "", fmt.Errorf("%s isn't a file Anywhere can keep", path)
	case info.Size() > Max:
		return "", errors.New("Files over 20 MB can't be shared")
	}
	data, err := os.ReadFile(full)
	if err != nil {
		return "", err
	}
	sum := sha256.Sum256(data)
	kept := filepath.Join(dir, hex.EncodeToString(sum[:6])+"-"+filepath.Base(full))
	if _, err := os.Stat(kept); err == nil {
		return kept, nil
	}
	if err := os.MkdirAll(dir, 0o700); err != nil {
		return "", err
	}
	if err := atomicfile.Write(kept, data, 0o600); err != nil {
		return "", err
	}
	return kept, nil
}

// Tool is what Claude names share_file: the tool of the anywhere server in
// pocketd's anywhere plugin.
const Tool = "mcp__plugin_anywhere_anywhere__share_file"

const latest = "2025-06-18"

var shareFile = map[string]any{
	"name": "share_file",
	"description": "Show the user a file: an image, chart, screenshot, document, log or report you made or found. " +
		"It appears as a card in their chat, on their Mac and their phone, and opens from there. " +
		"Anywhere keeps a copy, so share a file once it's final. Returns the path of that copy.",
	"inputSchema": map[string]any{
		"type": "object",
		"properties": map[string]any{
			"path":    map[string]any{"type": "string", "description": "The file to share: absolute, or relative to your working directory."},
			"caption": map[string]any{"type": "string", "description": "One line on what it is, shown with it."},
		},
		"required": []string{"path"},
	},
}

type request struct {
	ID     json.RawMessage `json:"id"`
	Method string          `json:"method"`
	Params json.RawMessage `json:"params"`
}

type response struct {
	JSONRPC string          `json:"jsonrpc"`
	ID      json.RawMessage `json:"id"`
	Result  any             `json:"result,omitempty"`
	Error   *rpcError       `json:"error,omitempty"`
}

type rpcError struct {
	Code    int    `json:"code"`
	Message string `json:"message"`
}

// Serve answers MCP requests from r on w, a JSON message a line, until r
// ends. share_file keeps its copies in dir.
func Serve(r io.Reader, w io.Writer, dir string) error {
	dec, enc := json.NewDecoder(r), json.NewEncoder(w)
	for {
		var req request
		if err := dec.Decode(&req); errors.Is(err, io.EOF) {
			return nil
		} else if err != nil {
			return err
		}
		// A notification wants no answer.
		if req.ID == nil {
			continue
		}
		res := response{JSONRPC: "2.0", ID: req.ID}
		res.Result, res.Error = answer(req, dir)
		if err := enc.Encode(res); err != nil {
			return err
		}
	}
}

func answer(req request, dir string) (any, *rpcError) {
	switch req.Method {
	case "initialize":
		var p struct {
			ProtocolVersion string `json:"protocolVersion"`
		}
		json.Unmarshal(req.Params, &p)
		if p.ProtocolVersion == "" {
			p.ProtocolVersion = latest
		}
		return map[string]any{
			"protocolVersion": p.ProtocolVersion,
			"capabilities":    map[string]any{"tools": map[string]any{}},
			"serverInfo":      map[string]string{"name": "anywhere", "version": "1.0.0"},
		}, nil
	case "ping":
		return map[string]any{}, nil
	case "tools/list":
		return map[string]any{"tools": []any{shareFile}}, nil
	case "tools/call":
		var p struct {
			Name      string `json:"name"`
			Arguments struct {
				Path string `json:"path"`
			} `json:"arguments"`
		}
		if json.Unmarshal(req.Params, &p) != nil || p.Name != "share_file" {
			return nil, &rpcError{-32602, "No such tool"}
		}
		kept, err := Keep(dir, p.Arguments.Path)
		if err != nil {
			return said(err.Error(), true), nil
		}
		return said(kept, false), nil
	}
	return nil, &rpcError{-32601, "No such method: " + req.Method}
}

// said is a tool's answer: text, and whether it reports a failure.
func said(text string, failed bool) map[string]any {
	return map[string]any{"content": []any{map[string]string{"type": "text", "text": text}}, "isError": failed}
}
