//! Nitrogen's TextMate palette (`Palette.cs`, `Palette.Ngr.cs`), entry for entry:
//! legend name, scope, and the standard type a client without palette tokens gets.

pub struct Entry {
    pub name: &'static str,
    pub scope: &'static str,
    pub fallback: Option<&'static str>,
}

pub static ENTRIES: &[Entry] = &[
    Entry {
        name: "operationWord",
        scope: "entity.name.function",
        fallback: Some("function"),
    },
    Entry {
        name: "declarationKeyword",
        scope: "storage.type",
        fallback: Some("keyword"),
    },
    Entry {
        name: "sectionKeyword",
        scope: "keyword.other",
        fallback: Some("keyword"),
    },
    Entry {
        name: "modifierWord",
        scope: "keyword.operator.word",
        fallback: Some("keyword"),
    },
    Entry {
        name: "constantWord",
        scope: "constant.language",
        fallback: Some("enumMember"),
    },
    Entry {
        name: "blockBracket",
        scope: "punctuation.section.block",
        fallback: None,
    },
    Entry {
        name: "parenthesis",
        scope: "punctuation.section.parens",
        fallback: None,
    },
    Entry {
        name: "squareBracket",
        scope: "punctuation.section.brackets",
        fallback: None,
    },
    Entry {
        name: "terminator",
        scope: "punctuation.terminator",
        fallback: None,
    },
    Entry {
        name: "separator",
        scope: "punctuation.separator",
        fallback: None,
    },
    Entry {
        name: "operator",
        scope: "keyword.operator",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrCommentBlock",
        scope: "comment.block",
        fallback: Some("comment"),
    },
    Entry {
        name: "ngrCommentLineDoubleSlash",
        scope: "comment.line.double-slash",
        fallback: Some("comment"),
    },
    Entry {
        name: "ngrConstantCharacterAny",
        scope: "constant.character.any",
        fallback: Some("enumMember"),
    },
    Entry {
        name: "ngrConstantCharacterEscape",
        scope: "constant.character.escape",
        fallback: Some("string"),
    },
    Entry {
        name: "ngrConstantNumericInteger",
        scope: "constant.numeric.integer",
        fallback: Some("number"),
    },
    Entry {
        name: "ngrConstantOtherDiagnosticCode",
        scope: "constant.other.diagnostic-code",
        fallback: Some("enumMember"),
    },
    Entry {
        name: "ngrEntityNameFunction",
        scope: "entity.name.function",
        fallback: Some("function"),
    },
    Entry {
        name: "ngrEntityNameFunctionAlternative",
        scope: "entity.name.function.alternative",
        fallback: Some("function"),
    },
    Entry {
        name: "ngrEntityNameFunctionCall",
        scope: "entity.name.function.call",
        fallback: Some("function"),
    },
    Entry {
        name: "ngrEntityNameFunctionRule",
        scope: "entity.name.function.rule",
        fallback: Some("function"),
    },
    Entry {
        name: "ngrEntityNameNamespace",
        scope: "entity.name.namespace",
        fallback: Some("namespace"),
    },
    Entry {
        name: "ngrEntityNameType",
        scope: "entity.name.type",
        fallback: Some("type"),
    },
    Entry {
        name: "ngrEntityNameTypeLanguage",
        scope: "entity.name.type.language",
        fallback: Some("type"),
    },
    Entry {
        name: "ngrEntityNameTypeQualified",
        scope: "entity.name.type.qualified",
        fallback: Some("type"),
    },
    Entry {
        name: "ngrEntityNameTypeToken",
        scope: "entity.name.type.token",
        fallback: Some("type"),
    },
    Entry {
        name: "ngrKeywordControlConditional",
        scope: "keyword.control.conditional",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordControlLoop",
        scope: "keyword.control.loop",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordControlMatch",
        scope: "keyword.control.match",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordControlWhere",
        scope: "keyword.control.where",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOperatorAlternative",
        scope: "keyword.operator.alternative",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorArrow",
        scope: "keyword.operator.arrow",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorAssignment",
        scope: "keyword.operator.assignment",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorComparison",
        scope: "keyword.operator.comparison",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorList",
        scope: "keyword.operator.list",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorNegation",
        scope: "keyword.operator.negation",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorOptional",
        scope: "keyword.operator.optional",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorPeg",
        scope: "keyword.operator.peg",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorRange",
        scope: "keyword.operator.range",
        fallback: Some("operator"),
    },
    Entry {
        name: "ngrKeywordOperatorWord",
        scope: "keyword.operator.word",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherAssociativity",
        scope: "keyword.other.associativity",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherAt",
        scope: "keyword.other.at",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherBuiltin",
        scope: "keyword.other.builtin",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherCheck",
        scope: "keyword.other.check",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherClause",
        scope: "keyword.other.clause",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherDeclares",
        scope: "keyword.other.declares",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherDirective",
        scope: "keyword.other.directive",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherFor",
        scope: "keyword.other.for",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherIn",
        scope: "keyword.other.in",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherLet",
        scope: "keyword.other.let",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherLowering",
        scope: "keyword.other.lowering",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherPrecedence",
        scope: "keyword.other.precedence",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherReferences",
        scope: "keyword.other.references",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherSetting",
        scope: "keyword.other.setting",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherSymbols",
        scope: "keyword.other.symbols",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrKeywordOtherUsing",
        scope: "keyword.other.using",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrPunctuationDefinitionCharacterClassBegin",
        scope: "punctuation.definition.character-class.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationDefinitionCharacterClassEnd",
        scope: "punctuation.definition.character-class.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationDefinitionDirective",
        scope: "punctuation.definition.directive",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationDefinitionStringBegin",
        scope: "punctuation.definition.string.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationDefinitionStringEnd",
        scope: "punctuation.definition.string.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionArgumentsBegin",
        scope: "punctuation.section.arguments.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionArgumentsEnd",
        scope: "punctuation.section.arguments.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionBlockBegin",
        scope: "punctuation.section.block.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionBlockEnd",
        scope: "punctuation.section.block.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionGroupBegin",
        scope: "punctuation.section.group.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionGroupEnd",
        scope: "punctuation.section.group.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionModuleBegin",
        scope: "punctuation.section.module.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionModuleEnd",
        scope: "punctuation.section.module.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionParametersBegin",
        scope: "punctuation.section.parameters.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionParametersEnd",
        scope: "punctuation.section.parameters.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionSemanticsBegin",
        scope: "punctuation.section.semantics.begin",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSectionSemanticsEnd",
        scope: "punctuation.section.semantics.end",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSeparatorComma",
        scope: "punctuation.separator.comma",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSeparatorLabel",
        scope: "punctuation.separator.label",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSeparatorList",
        scope: "punctuation.separator.list",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSeparatorMessage",
        scope: "punctuation.separator.message",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationSeparatorType",
        scope: "punctuation.separator.type",
        fallback: None,
    },
    Entry {
        name: "ngrPunctuationTerminator",
        scope: "punctuation.terminator",
        fallback: None,
    },
    Entry {
        name: "ngrStorageModifierDirection",
        scope: "storage.modifier.direction",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageModifierExtend",
        scope: "storage.modifier.extend",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageModifierExtensible",
        scope: "storage.modifier.extensible",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageModifierFlag",
        scope: "storage.modifier.flag",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeFunction",
        scope: "storage.type.function",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeLanguage",
        scope: "storage.type.language",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeModule",
        scope: "storage.type.module",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeProperty",
        scope: "storage.type.property",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeSymbol",
        scope: "storage.type.symbol",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeSyntax",
        scope: "storage.type.syntax",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStorageTypeToken",
        scope: "storage.type.token",
        fallback: Some("keyword"),
    },
    Entry {
        name: "ngrStringQuotedDouble",
        scope: "string.quoted.double",
        fallback: Some("string"),
    },
    Entry {
        name: "ngrStringQuotedOtherTemplate",
        scope: "string.quoted.other.template",
        fallback: Some("string"),
    },
    Entry {
        name: "ngrStringQuotedSingle",
        scope: "string.quoted.single",
        fallback: Some("string"),
    },
    Entry {
        name: "ngrSupportFunctionBuiltin",
        scope: "support.function.builtin",
        fallback: Some("function"),
    },
    Entry {
        name: "ngrSupportTypeSymbolKind",
        scope: "support.type.symbol-kind",
        fallback: Some("type"),
    },
    Entry {
        name: "ngrVariableLanguageWildcard",
        scope: "variable.language.wildcard",
        fallback: Some("variable"),
    },
    Entry {
        name: "ngrVariableOtherDefinition",
        scope: "variable.other.definition",
        fallback: Some("variable"),
    },
    Entry {
        name: "ngrVariableOtherMember",
        scope: "variable.other.member",
        fallback: Some("variable"),
    },
    Entry {
        name: "ngrVariableOtherMemberLabel",
        scope: "variable.other.member.label",
        fallback: Some("variable"),
    },
    Entry {
        name: "ngrVariableOtherProperty",
        scope: "variable.other.property",
        fallback: Some("property"),
    },
    Entry {
        name: "ngrVariableOtherSplice",
        scope: "variable.other.splice",
        fallback: Some("variable"),
    },
    Entry {
        name: "ngrVariableParameter",
        scope: "variable.parameter",
        fallback: Some("parameter"),
    },
];

/// The general entry for a literal role (the first eleven entries).
pub fn of(role: crate::roles::Role) -> &'static Entry {
    use crate::roles::Role::*;
    &ENTRIES[match role {
        Operation => 0,
        DeclarationKeyword => 1,
        SectionKeyword => 2,
        ModifierWord => 3,
        ConstantWord => 4,
        BlockBracket => 5,
        Parenthesis => 6,
        SquareBracket => 7,
        Terminator => 8,
        Separator => 9,
        Operator => 10,
    }]
}

/// The `.ngr` entry for a TextMate scope.
pub fn ngr(scope: &str) -> Option<&'static Entry> {
    ENTRIES
        .iter()
        .find(|e| e.name.starts_with("ngr") && e.scope == scope)
}

/// The `.ngr` scope of a literal in a grammar-frontend rule (its last name
/// segment): Nitrogen's `NgrLiterals`, by rule where the spelling is not
/// enough, then by spelling. Rules are Nitride's frontend rules.
pub fn ngr_literal(rule: &str, literal: &str) -> Option<&'static Entry> {
    let by_rule = match (rule, literal) {
        ("Question", "?") => Some("keyword.operator.optional"),
        ("KindGroup", "|") => Some("keyword.operator.peg"),
        ("KindGroup", "(") | ("Group", "(") | ("EmptyGroup", "(") => {
            Some("punctuation.section.group.begin")
        }
        ("KindGroup", ")") | ("Group", ")") | ("EmptyGroup", ")") => {
            Some("punctuation.section.group.end")
        }
        ("Separator", ";") => Some("punctuation.separator.list"),
        ("Module", "module") => Some("storage.type.module"),
        ("Module", "{") => Some("punctuation.section.module.begin"),
        ("Module", "}") => Some("punctuation.section.module.end"),
        ("Class" | "ClassValue", "[") => Some("punctuation.definition.character-class.begin"),
        ("Class" | "ClassValue", "]") => Some("punctuation.definition.character-class.end"),
        ("Any", ".") => Some("constant.character.any"),
        ("Labelled", ":") => Some("punctuation.separator.label"),
        ("Helper" | "Function" | "Captures", "(") => Some("punctuation.section.parameters.begin"),
        ("Helper" | "Function" | "Captures", ")") => Some("punctuation.section.parameters.end"),
        ("Call" | "HelperCall" | "Template" | "Expand" | "ComputedCall", "(") => {
            Some("punctuation.section.arguments.begin")
        }
        ("Call" | "HelperCall" | "Template" | "Expand" | "ComputedCall", ")") => {
            Some("punctuation.section.arguments.end")
        }
        _ => None,
    };
    let scope = by_rule.or_else(|| {
        Some(match literal {
            "syntax" => "storage.type.syntax",
            "token" => "storage.type.token",
            "function" => "storage.type.function",
            "property" => "storage.type.property",
            "symbol" => "storage.type.symbol",
            "language" => "storage.type.language",
            "module" => "storage.type.module",
            "out" | "in" => "storage.modifier.direction",
            "extensible" => "storage.modifier.extensible",
            "extend" => "storage.modifier.extend",
            "hover" => "storage.modifier.flag",
            "lowers" | "rewrites" | "scope" | "except" | "sequential" | "export" | "dynamic" => {
                "keyword.other.clause"
            }
            "declares" => "keyword.other.declares",
            "references" => "keyword.other.references",
            "precedence" => "keyword.other.precedence",
            "left" | "right" => "keyword.other.associativity",
            "builtin" => "keyword.other.builtin",
            "check" => "keyword.other.check",
            "symbols" => "keyword.other.symbols",
            "for" => "keyword.other.for",
            "using" => "keyword.other.using",
            "let" => "keyword.other.let",
            "initializer" | "literal" | "operation" | "reference" | "type" | "text"
            | "sequence" | "optional" | "value" | "repeat" | "template" | "expand" => {
                "keyword.other.lowering"
            }
            "start" | "namespace" | "grammars" | "sources" | "extensions" | "usings" | "tokens"
            | "types" => "keyword.other.setting",
            "match" => "keyword.control.match",
            "if" | "then" | "else" => "keyword.control.conditional",
            "where" => "keyword.control.where",
            "choice" | "seq" => "keyword.control.loop",
            "and" | "or" | "not" => "keyword.operator.word",
            "_" => "variable.language.wildcard",
            "=" => "keyword.operator.assignment",
            "=>" => "keyword.operator.arrow",
            ".." => "keyword.operator.range",
            "^" => "keyword.operator.negation",
            "," => "punctuation.separator.comma",
            ";" => "punctuation.terminator",
            "/" | "*" | "+" | "!" | "&" | "?" => "keyword.operator.peg",
            "|" => "keyword.operator.alternative",
            "{" => "punctuation.section.block.begin",
            "}" => "punctuation.section.block.end",
            ":" => "punctuation.separator.type",
            _ => return None,
        })
    });
    scope.and_then(ngr)
}
