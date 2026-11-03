; zed-extensions/astro languages/astro/highlights.scm (Apache-2.0)
(tag_name) @tag

((tag_name) @tag.component
  (#match? @tag.component "^[A-Z]"))

(erroneous_end_tag_name) @keyword

(doctype) @tag.doctype

(attribute_name) @attribute

(attribute_value) @string

(comment) @comment

[
  (attribute_value)
  (quoted_attribute_value)
] @string

"=" @operator

[
  "{"
  "}"
] @punctuation.bracket

[
  "<"
  ">"
  "</"
  "/>"
] @tag.delimiter
