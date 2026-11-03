; zed-extensions/vue languages/vue/injections.scm (Apache-2.0), in tree-sitter's injection captures.
; jsx is set to tsx because gpui-kit has no grammar by that name.

; <script>
((script_element
  (start_tag) @_no_lang
  (raw_text) @injection.content)
  (#not-match? @_no_lang "lang=")
  (#set! injection.language "javascript"))

; <script lang="js">
((script_element
  (start_tag
    (attribute
      (attribute_name) @_lang
      (quoted_attribute_value
        (attribute_value) @_js)))
  (raw_text) @injection.content)
  (#eq? @_lang "lang")
  (#eq? @_js "js")
  (#set! injection.language "javascript"))

; <script lang="ts">
((script_element
  (start_tag
    (attribute
      (attribute_name) @_lang
      (quoted_attribute_value
        (attribute_value) @_ts)))
  (raw_text) @injection.content)
  (#eq? @_lang "lang")
  (#eq? @_ts "ts")
  (#set! injection.language "typescript"))

; <script lang="tsx">
; <script lang="jsx">
((script_element
  (start_tag
    (attribute
      (attribute_name) @_attr
      (quoted_attribute_value
        (attribute_value) @_tsx)))
  (raw_text) @injection.content)
  (#eq? @_attr "lang")
  (#any-of? @_tsx "tsx" "jsx")
  (#set! injection.language "tsx"))

; {{ }}
((interpolation
  (raw_text) @injection.content)
  (#set! injection.language "typescript"))

; v-
(directive_attribute
  (quoted_attribute_value
    (attribute_value) @injection.content
    (#set! injection.language "typescript")))

; Vue <style lang="css"> injections
(style_element
  (start_tag
    (attribute
      (attribute_name) @_attr_name
      (#eq? @_attr_name "lang")
      (quoted_attribute_value
        (attribute_value) @injection.language)))
  (raw_text) @injection.content)

; Vue <style> css injections (no lang attribute)
(style_element
  (start_tag
    (attribute
      (attribute_name) @_attr_name)*)
  (raw_text) @injection.content
  (#not-any-of? @_attr_name "lang")
  (#set! injection.language "css"))

; Vue custom blocks (e.g <i18n lang="json">)
; See https://vuejs.org/api/sfc-spec.html#custom-blocks
(element
  (start_tag
    (tag_name) @_tag
    (attribute
      (attribute_name) @_attr
      (#eq? @_attr "lang")
      (quoted_attribute_value
        (attribute_value) @injection.language)))
  (#not-any-of? @_tag "template" "script" "style")
  (text) @injection.content)

; <!-- -->
; Make comment as html
((comment) @injection.content
  (#set! injection.language "html"))
