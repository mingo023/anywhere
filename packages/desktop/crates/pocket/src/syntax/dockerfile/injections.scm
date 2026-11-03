; zed-extensions/dockerfile languages/dockerfile/injections.scm (Apache-2.0), in tree-sitter's injection captures.
((comment) @injection.content
  (#set! injection.language "comment"))

((shell_command) @injection.content
  (#set! injection.language "bash"))
