pub fn get_json_schema_gbnf() -> &'static str {
    // A simplified GBNF grammar that restricts response to valid JSON structures.
    // It enforces a structured array of extraction insights (e.g. inferred relations/calls/dependencies).
    r#"root   ::= object
value  ::= object | array | string | number | "true" | "false" | "null"
object ::= "{" space? ( pair ( "," space? pair )* )? space? "}"
pair   ::= string space? ":" space? value
array  ::= "[" space? ( value ( "," space? value )* )? space? "]"
string ::= "\"" ( [^"\\] | "\\" ( ["\\/bfnrt] | "u" [0-9a-fA-F]{4} ) )* "\""
number ::= "-"? ( "0" | [1-9] [0-9]* ) ( "." [0-9]+ )? ( [eE] [+-]? [0-9]+ )?
space  ::= [ \t\n\r]+
"#
}
