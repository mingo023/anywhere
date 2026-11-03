; zed-extensions/vue languages/vue/highlights.scm (Apache-2.0); the grammar now names directive_argument directive_value.
(comment) @comment

((tag_name) @tag
  (#match? @tag "^[a-z]"))

((tag_name) @tag @tag.component.type.constructor
  (#match? @tag "^[A-Z]"))

(attribute) @attribute

(directive_attribute) @property

(quoted_attribute_value) @string

(interpolation) @punctuation.special

(raw_text) @embedded

((directive_name) @punctuation.delimiter
  (#any-of? @punctuation.delimiter ":" "@" "#"))

((directive_name) @keyword.directive
  (#match? @keyword.directive "^v-"))

(directive_value) @attribute

(start_tag) @tag

(end_tag) @tag

(self_closing_tag) @tag

[
  "="
  ":"
] @punctuation.delimiter

[
  "<"
  ">"
  "</"
  "/>"
] @punctuation.bracket
