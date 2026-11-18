(comment) @comment
(signature) @preproc
(keyword) @keyword
(boolean) @boolean
(number) @number
(string) @string
(character) @string
(verbatim_string) @string
(raw_string) @string
(escape_sequence) @string.escape
(operator) @operator
(punctuation) @punctuation.delimiter

["syntax" "catalog" "module" "binding" "language" "token" "type"
 "operation" "function" "pattern" "context"] @keyword

(module_header name: (qualified_name) @type)
(binding_header name: (qualified_name) @type)
(language_header name: (identifier) @type)
(declaration_header name: (identifier) @type)
(declaration_header ["operation" "function"] name: (identifier) @function)

["{" "}" "(" ")" "[" "]"] @punctuation.bracket
