//! Per-language node-kind specs. Each pairs a [`LangSpec`] with its tree-sitter grammar. To add
//! a tree-sitter language: add its grammar dependency and one entry here. (COBOL is handled by a
//! bespoke adapter — see [`crate::cobol`].)

use crate::LangSpec;
use tree_sitter::Language;

const PYTHON: LangSpec = LangSpec {
    id: "python",
    extensions: &["py", "pyi"],
    function_kinds: &["function_definition"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "elif_clause",
        "for_statement",
        "while_statement",
        "except_clause",
        "with_statement",
        "conditional_expression",
        "boolean_operator",
        "assert_statement",
    ],
    definition_kinds: &["function_definition", "class_definition"],
    call_kinds: &["call"],
    callee_field: Some("function"),
    sinks: &[
        "eval",
        "exec",
        "os.system",
        "os.popen",
        "subprocess.call",
        "subprocess.run",
        "subprocess.Popen",
        "pickle.loads",
        "yaml.load",
        "marshal.loads",
        "__import__",
    ],
};

const RUST: LangSpec = LangSpec {
    id: "rust",
    extensions: &["rs"],
    function_kinds: &["function_item"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_expression",
        "match_arm",
        "while_expression",
        "for_expression",
        "loop_expression",
    ],
    definition_kinds: &["function_item"],
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: &["Command::new"],
};

const GO: LangSpec = LangSpec {
    id: "go",
    extensions: &["go"],
    function_kinds: &["function_declaration", "method_declaration"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "expression_case",
        "type_case",
        "communication_case",
        "select_statement",
    ],
    definition_kinds: &["function_declaration", "method_declaration"],
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: &["exec.Command", "exec.CommandContext", "os.StartProcess"],
};

const RUBY: LangSpec = LangSpec {
    id: "ruby",
    extensions: &["rb"],
    function_kinds: &["method", "singleton_method"],
    name_field: Some("name"),
    decision_kinds: &[
        "if",
        "elsif",
        "unless",
        "while",
        "until",
        "for",
        "when",
        "case",
        "rescue",
        "and",
        "or",
        "conditional",
    ],
    definition_kinds: &["method", "singleton_method"],
    call_kinds: &["call", "method_call", "command", "command_call"],
    callee_field: Some("method"),
    sinks: &[
        "eval",
        "system",
        "exec",
        "instance_eval",
        "class_eval",
        "send",
    ],
};

const JAVA: LangSpec = LangSpec {
    id: "java",
    extensions: &["java"],
    function_kinds: &["method_declaration", "constructor_declaration"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "enhanced_for_statement",
        "while_statement",
        "do_statement",
        "switch_label",
        "catch_clause",
        "ternary_expression",
    ],
    definition_kinds: &["method_declaration"],
    call_kinds: &["method_invocation"],
    callee_field: Some("name"),
    sinks: &["exec", "eval"],
};

const JAVASCRIPT: LangSpec = LangSpec {
    id: "javascript",
    extensions: &["js", "jsx", "mjs", "cjs"],
    function_kinds: &[
        "function_declaration",
        "function_expression",
        "arrow_function",
        "method_definition",
        "generator_function_declaration",
    ],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "for_in_statement",
        "while_statement",
        "do_statement",
        "switch_case",
        "catch_clause",
        "ternary_expression",
    ],
    definition_kinds: &["function_declaration", "class_declaration"],
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: &["eval", "exec", "execSync", "spawn"],
};

// TypeScript and TSX use *different* tree-sitter grammars — the TS grammar cannot parse JSX,
// so `.tsx` must use LANGUAGE_TSX. The node kinds are otherwise identical, so they share one set.
const TS_FUNCTION_KINDS: &[&str] = &[
    "function_declaration",
    "function_expression",
    "arrow_function",
    "method_definition",
    "generator_function_declaration",
];
const TS_DECISION_KINDS: &[&str] = &[
    "if_statement",
    "for_statement",
    "for_in_statement",
    "while_statement",
    "do_statement",
    "switch_case",
    "catch_clause",
    "ternary_expression",
];
const TS_DEFINITION_KINDS: &[&str] = &["function_declaration", "class_declaration"];
const TS_SINKS: &[&str] = &["eval", "exec", "execSync", "spawn"];

const TYPESCRIPT: LangSpec = LangSpec {
    id: "typescript",
    extensions: &["ts", "mts", "cts"],
    function_kinds: TS_FUNCTION_KINDS,
    name_field: Some("name"),
    decision_kinds: TS_DECISION_KINDS,
    definition_kinds: TS_DEFINITION_KINDS,
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: TS_SINKS,
};

const TSX: LangSpec = LangSpec {
    id: "tsx",
    extensions: &["tsx"],
    function_kinds: TS_FUNCTION_KINDS,
    name_field: Some("name"),
    decision_kinds: TS_DECISION_KINDS,
    definition_kinds: TS_DEFINITION_KINDS,
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: TS_SINKS,
};

const C: LangSpec = LangSpec {
    id: "c",
    extensions: &["c", "h"],
    function_kinds: &["function_definition"],
    name_field: None, // C nests the name inside declarators; complexity still computes.
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "case_statement",
        "conditional_expression",
    ],
    definition_kinds: &[], // name not directly addressable; dead-code disabled for C
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: &[
        "system", "popen", "exec", "execl", "execlp", "execvp", "gets", "strcpy", "sprintf",
    ],
};

const CPP: LangSpec = LangSpec {
    id: "cpp",
    extensions: &["cpp", "cc", "cxx", "hpp", "hh"],
    function_kinds: &["function_definition"],
    name_field: None,
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "for_range_loop",
        "while_statement",
        "do_statement",
        "case_statement",
        "conditional_expression",
        "catch_clause",
    ],
    definition_kinds: &[],
    call_kinds: &["call_expression"],
    callee_field: Some("function"),
    sinks: &[
        "system", "popen", "exec", "execl", "execlp", "execvp", "strcpy", "sprintf",
    ],
};

const CSHARP: LangSpec = LangSpec {
    id: "csharp",
    extensions: &["cs"],
    function_kinds: &[
        "method_declaration",
        "constructor_declaration",
        "local_function_statement",
    ],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "for_each_statement",
        "while_statement",
        "do_statement",
        "switch_section",
        "catch_clause",
        "conditional_expression",
    ],
    definition_kinds: &["method_declaration", "class_declaration"],
    call_kinds: &["invocation_expression"],
    callee_field: Some("function"),
    sinks: &["Process.Start", "eval"],
};

const PHP: LangSpec = LangSpec {
    id: "php",
    extensions: &["php"],
    function_kinds: &["function_definition", "method_declaration"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "foreach_statement",
        "while_statement",
        "do_statement",
        "switch_block",
        "catch_clause",
        "conditional_expression",
    ],
    definition_kinds: &["function_definition", "class_declaration"],
    call_kinds: &["function_call_expression", "member_call_expression"],
    callee_field: Some("function"),
    sinks: &[
        "eval",
        "exec",
        "system",
        "shell_exec",
        "passthru",
        "popen",
        "proc_open",
        "assert",
    ],
};

const BASH: LangSpec = LangSpec {
    id: "bash",
    extensions: &["sh", "bash"],
    function_kinds: &["function_definition"],
    name_field: Some("name"),
    decision_kinds: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "case_item",
        "elif_clause",
    ],
    definition_kinds: &["function_definition"],
    call_kinds: &["command"],
    callee_field: Some("name"),
    sinks: &["eval", "exec", "source"],
};

/// All shipped tree-sitter (spec, grammar) pairs.
pub fn all() -> Vec<(LangSpec, Language)> {
    vec![
        (PYTHON, tree_sitter_python::LANGUAGE.into()),
        (RUST, tree_sitter_rust::LANGUAGE.into()),
        (GO, tree_sitter_go::LANGUAGE.into()),
        (RUBY, tree_sitter_ruby::LANGUAGE.into()),
        (JAVA, tree_sitter_java::LANGUAGE.into()),
        (JAVASCRIPT, tree_sitter_javascript::LANGUAGE.into()),
        (
            TYPESCRIPT,
            tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        ),
        (TSX, tree_sitter_typescript::LANGUAGE_TSX.into()),
        (C, tree_sitter_c::LANGUAGE.into()),
        (CPP, tree_sitter_cpp::LANGUAGE.into()),
        (CSHARP, tree_sitter_c_sharp::LANGUAGE.into()),
        (PHP, tree_sitter_php::LANGUAGE_PHP.into()),
        (BASH, tree_sitter_bash::LANGUAGE.into()),
    ]
}
