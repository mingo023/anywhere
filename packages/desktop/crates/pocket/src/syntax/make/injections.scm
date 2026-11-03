; caius/zed-make languages/make/injections.scm (MIT), in tree-sitter's injection captures.
((comment) @injection.content
  (#set! injection.language "comment"))

((shell_text) @injection.content
  (#set! injection.language "bash"))

((shell_command) @injection.content
  (#set! injection.language "bash"))
