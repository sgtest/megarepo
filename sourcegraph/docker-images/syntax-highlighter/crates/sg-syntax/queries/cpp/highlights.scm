
"--" @operator
"-" @operator
"-=" @operator
"->" @operator
"=" @operator
"!=" @operator
"*" @operator
"&" @operator
"&&" @operator
"+" @operator
"++" @operator
"+=" @operator
"<" @operator
"==" @operator
">" @operator
"||" @operator

"." @delimiter
";" @delimiter

(string_literal) @string
(system_lib_string) @string

(null) @constant.null
(number_literal) @number
(char_literal) @character
(true) @boolean
(false) @boolean

(call_expression
  function: (identifier) @identifier.function)
(call_expression
  function: (field_expression
    field: (field_identifier) @identifier.function))
(function_declarator
  declarator: (identifier) @identifier.function)
(preproc_function_def
  name: (identifier) @identifier.function)

(field_identifier) @identifier.attribute
(statement_identifier) @identifier.attribute
(type_identifier) @type
(static_assert_declaration ("static_assert") @identifier.builtin)
(primitive_type) @type.builtin
(sized_type_specifier) @type

((identifier) @constant
 (#match? @constant "^[A-Z][A-Z\\d_]*$"))

(identifier) @identifier
(namespace_identifier) @identifier.module

(this) @constant.builtin
(comment) @comment
(operator_name) @identifier.operator
(auto) @keyword

[
  "#define"
  "#elif"
  "#else"
  "#endif"
  "#if"
  "#ifdef"
  "#ifndef"
  "#include"
  "break"
  "case"
  "const"
  "continue"
  "co_await"
  "co_return"
  "co_yield"
  "default"
  "delete"
  "class"
  "public"
  "protected"
  "private"
  "final"
  "virtual"
  "friend"
  "goto"
  "do"
  "else"
  "enum"
  "explicit"
  "extern"
  "for"
  "if"
  "try"
  "catch"
  "throw"
  "inline"
  "namespace"
  "new"
  "noexcept"
  "return"
  "sizeof"
  "static"
  "struct"
  "decltype"
  "switch"
  "template"
  "typedef"
  "typename"
  "union"
  "using"
  "volatile"
  "constexpr"
  "while"
  (preproc_directive)
] @keyword
