use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree};
use serde_json::{Value, json};

pub struct Module {
    pub name: String,
    pub imports: Vec<String>,
    pub symbols: Vec<String>,
    pub builtins: Vec<(String, String)>,
    pub helpers: Vec<Helper>,
    pub functions: Vec<Function>,
    pub rules: Vec<Rule>,
    pub extensions: Vec<(String, Vec<Rule>)>,
    pub source: usize,
    pub span: [usize; 2],
}
/// A typed quote helper: `syntax function Name(P: Category) : Category = <[ ... ]>`.
/// A match helper keeps its fallback as `body`, as Nitrogen's model does.
pub struct Helper {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub result: (String, [usize; 2]),
    pub body: String,
    pub selector: Option<String>,
    pub arms: Vec<Arm>,
    pub span: [usize; 2],
}
/// A grammar function, `function Name(p: Grammar.Int, ...) : Grammar.Rule = let ...; Body;`.
/// Its body uses the expression model plus `call`, `if`, `comprehension` and
/// `valueclass` nodes; values are JSON objects tagged by `"v"`.
pub struct Function {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub result: (String, [usize; 2]),
    pub lets: Vec<(String, Value)>,
    pub body: Value,
    pub span: [usize; 2],
}
pub struct Parameter {
    pub name: String,
    pub category: String,
    pub span: [usize; 2],
}
pub struct Arm {
    pub captures: Vec<Parameter>,
    pub pattern: String,
    pub result: String,
}

#[derive(Clone)]
pub struct Rule {
    pub name: String,
    pub body: Value,
    pub lexical: bool,
    pub clauses: Vec<Value>,
    pub block: bool,
    pub except: Vec<String>,
    pub precedence: Option<u32>,
    pub associativity: Option<String>,
    pub alternatives: Option<Vec<Rule>>,
    pub transparent: bool,
    pub span: [usize; 2],
}

// Recognition comes exclusively from the generated admitted grammar. This
// decoder projects the resulting syntax into the preparation model; it does not
// consume tokens, choose alternatives or backtrack.
/// Decodes one grammar source. Nitrogen's C# semantic members are errors,
/// unless `tolerant`, when each is skipped and returned as a warning.
pub fn parse(
    source: &Source,
    index: usize,
    tolerant: bool,
) -> Result<(Vec<Module>, Vec<PrepareError>), PrepareError> {
    let tree = syntax(source)?;
    let decoder = Decoder {
        source,
        tree: &tree,
        tolerant,
        skipped: Default::default(),
    };
    // Nodes are postorder. Bound the combined model depth before constructing
    // recursive JSON, cloning separated-list items, or invoking normalization.
    // Syntax wrappers conservatively cover the object/array wrappers introduced
    // by list desugaring; each postfix adds another model wrapper of its own.
    let mut depths = Vec::with_capacity(tree.nodes.len());
    for id in 0..tree.nodes.len() as u32 {
        let children = tree.children(id);
        let nested = children
            .iter()
            .map(|e| depths[e.node as usize])
            .max()
            .unwrap_or(0);
        let postfix = if decoder.kind(id) == "Postfixed" {
            children.iter().filter(|e| tree.label(e) == "Ops").count()
        } else {
            0
        };
        let depth = nested + postfix + 1;
        if depth > 128 {
            return Err(decoder.error(id, "grammar nesting exceeds 128"));
        }
        depths.push(depth);
        // Recognition is generated; UTF-16 pairing is a model constraint,
        // including captured literals within opaque semantic regions.
        match decoder.kind(id) {
            "String" | "OpaqueCharLiteral" => {
                decoder.literal(id)?;
            }
            "Char" => {
                crate::literal::decode_unit(decoder.source, decoder.span(id))?;
            }
            _ => {}
        }
    }
    drop(depths);
    let modules = decoder.modules(index)?;
    Ok((modules, decoder.skipped.into_inner()))
}

/// The grammar frontend's tree of one source.
fn syntax(source: &Source) -> Result<SyntaxTree<'static>, PrepareError> {
    nitride_core::parse_with_options(
        &crate::grammar_seed::LANGUAGE,
        &source.text,
        Limits {
            parser_frames: 512,
            ..Limits::default()
        },
        nitride_core::ParseOptions {
            strict_comments: true,
            // Literal validation inspects quoted text inside opaque blocks.
            token_children: true,
            alias_nodes: true,
        },
    )
    .map_err(|e| {
        source.error(
            e.span.map(|v| v as usize),
            if e.category == "limit" {
                "grammar nesting/work limit exceeded".into()
            } else {
                e.message
            },
        )
    })
}

/// A grammar parsed for editor structure: while it fails to parse, the line
/// of the error is blanked (braces kept, so blocks stay balanced) and parsing
/// retried, so the rest of a grammar under edit still yields its structure.
fn tolerant(source: &Source) -> Result<Source, PrepareError> {
    let mut text = source.text.clone();
    let mut first = None;
    for _ in 0..32 {
        let attempt = Source {
            path: source.path.clone(),
            text: text.clone(),
        };
        match syntax(&attempt) {
            Ok(_) => return Ok(attempt),
            Err(error) => {
                let at = error.span[0].min(text.len());
                let start = text[..at].rfind('\n').map_or(0, |i| i + 1);
                let end = text[at..].find('\n').map_or(text.len(), |i| at + i);
                let blanked: String = text[start..end]
                    .chars()
                    .map(|c| if matches!(c, '{' | '}') { c } else { ' ' })
                    .collect();
                if blanked == text[start..end] {
                    // A file that just ends mid-block (being typed): close its
                    // blocks once, else give up.
                    let open = text
                        .matches('{')
                        .count()
                        .saturating_sub(text.matches('}').count());
                    if open == 0 || text.ends_with('}') {
                        return Err(first.unwrap_or(error));
                    }
                    first.get_or_insert(error);
                    text.push_str(&"\n}".repeat(open));
                    continue;
                }
                first.get_or_insert(error);
                text.replace_range(start..end, &blanked);
            }
        }
    }
    Err(first.expect("an error was seen"))
}

/// Whether a grammar parses as written.
pub fn parses(source: &Source) -> bool {
    syntax(source).is_ok()
}

/// Names used in a grammar, with the module each is used in: rules named by
/// element references, `extend` targets, function calls and parameter and
/// result types, and modules named
/// by `using` and by the qualifier of a qualified rule name.
pub fn references(source: &Source) -> Result<Vec<crate::GrammarReference>, PrepareError> {
    let source = &tolerant(source)?;
    let tree = syntax(source)?;
    let decoder = Decoder {
        source,
        tree: &tree,
        tolerant: true,
        skipped: Default::default(),
    };
    let modules: Vec<_> = decoder
        .fields(tree.root, "Modules")
        .into_iter()
        .map(|m| (decoder.span(m), decoder.named(m, "Name")))
        .collect();
    let mut found = Vec::new();
    for id in 0..tree.nodes.len() as u32 {
        let (kind, span) = match decoder.kind(id) {
            "Reference" | "Extend" => ("rule", decoder.span(decoder.field(id, "Name"))),
            "HelperCall" => ("rule", decoder.span(decoder.field(id, "Target"))),
            // Parameter, capture and result types of helpers and functions.
            "Parameter" => ("rule", decoder.span(decoder.field(id, "Category"))),
            "Helper" | "Function" => ("rule", decoder.span(decoder.field(id, "Result"))),
            // A call head is one token, `Name(`: the name is all but the parenthesis.
            "FunctionCall" | "CallValue" => {
                let [start, end] = decoder.span(decoder.field(id, "Head"));
                ("rule", [start, end - 1])
            }
            "Using" => ("module", decoder.span(decoder.field(id, "Name"))),
            _ => continue,
        };
        let module = modules
            .iter()
            .find(|(m, _)| m[0] <= span[0] && span[1] <= m[1])
            .map(|(_, name)| name.clone())
            .unwrap_or_default();
        let name = &source.text[span[0]..span[1]];
        if let Some(dot) = name.rfind('.').filter(|_| kind == "rule") {
            found.push(crate::GrammarReference {
                kind: "module",
                name: name[..dot].into(),
                span: [span[0], span[0] + dot],
                module: module.clone(),
            });
        }
        found.push(crate::GrammarReference {
            kind,
            name: name.into(),
            span,
            module,
        });
    }
    found.sort_by_key(|r| r.span[0]);
    Ok(found)
}

/// Helper parameters, match captures and element labels, with their uses.
pub fn locals(source: &Source) -> Result<Vec<crate::GrammarLocal>, PrepareError> {
    let source = &tolerant(source)?;
    let tree = syntax(source)?;
    let decoder = Decoder {
        source,
        tree: &tree,
        tolerant: true,
        skipped: Default::default(),
    };
    let mut found = Vec::new();
    for id in 0..tree.nodes.len() as u32 {
        match decoder.kind(id) {
            "Helper" => decoder.helper_locals(id, &mut found),
            "SyntaxRule" | "Alternative" => decoder.label_locals(id, &mut found),
            _ => {}
        }
    }
    found.sort_by_key(|l| l.definitions[0][0]);
    Ok(found)
}

/// Records a use of `name` with the first local that binds it.
fn bind(locals: &mut [crate::GrammarLocal], name: &str, span: [usize; 2]) -> bool {
    match locals.iter_mut().find(|l| l.name == name) {
        Some(local) => {
            local.uses.push(span);
            true
        }
        None => false,
    }
}

/// Outline items: modules and the rules, tokens and functions they declare.
pub fn outline(source: &Source) -> Result<Vec<crate::GrammarItem>, PrepareError> {
    let source = &tolerant(source)?;
    let tree = syntax(source)?;
    let decoder = Decoder {
        source,
        tree: &tree,
        tolerant: true,
        skipped: Default::default(),
    };
    let item = |kind: &'static str, id: u32, children| {
        let name = decoder.field(id, "Name");
        crate::GrammarItem {
            kind,
            name: decoder.text(name).into(),
            span: decoder.span(id),
            name_span: decoder.span(name),
            children,
        }
    };
    Ok(decoder
        .fields(tree.root, "Modules")
        .into_iter()
        .map(|module| {
            let members = decoder
                .fields(module, "Members")
                .into_iter()
                .filter_map(|member| {
                    let m = decoder.first(member);
                    let kind = match decoder.kind(m) {
                        "TokenRule" => "token",
                        "SyntaxRule" => "syntax",
                        "ExtensibleRule" => "extensible",
                        "Extend" => "extend",
                        "Function" => "function",
                        "Helper" => "helper",
                        _ => return None,
                    };
                    // Named alternatives are rules too.
                    let alternatives = decoder
                        .fields(m, "Alternatives")
                        .into_iter()
                        .filter_map(|a| decoder.tree.field(a, "Named"))
                        .map(|named| item("alternative", named, vec![]))
                        .collect();
                    Some(item(kind, m, alternatives))
                })
                .collect();
            item("module", module, members)
        })
        .collect())
}

struct Decoder<'a> {
    source: &'a Source,
    tree: &'a SyntaxTree<'static>,
    tolerant: bool,
    skipped: std::cell::RefCell<Vec<PrepareError>>,
}
impl Decoder<'_> {
    fn kind(&self, id: u32) -> &str {
        crate::grammar_seed::LANGUAGE.rules[self.tree.nodes[id as usize].rule as usize]
            .name
            .rsplit('.')
            .next()
            .unwrap()
    }
    fn span(&self, id: u32) -> [usize; 2] {
        self.tree.nodes[id as usize].span.map(|v| v as usize)
    }
    fn text(&self, id: u32) -> &str {
        self.tree.text(id).unwrap()
    }
    fn error(&self, id: u32, message: &str) -> PrepareError {
        self.source.error(self.span(id), message)
    }
    /// A C# semantic member: skipped with a warning when tolerant, else an error.
    fn semantic_member(&self, id: u32) -> Result<(), PrepareError> {
        let diagnostic = self.error(id, "C# semantic member is not admitted");
        if !self.tolerant {
            return Err(diagnostic);
        }
        self.skipped.borrow_mut().push(diagnostic);
        Ok(())
    }
    fn field(&self, id: u32, label: &str) -> u32 {
        self.tree.field(id, label).expect("generated grammar field")
    }
    fn fields(&self, id: u32, label: &str) -> Vec<u32> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == label)
            .map(|e| e.node)
            .collect()
    }
    fn first(&self, id: u32) -> u32 {
        self.tree.children(id)[0].node
    }
    fn named(&self, id: u32, label: &str) -> String {
        self.text(self.field(id, label)).into()
    }
    /// Descendants of `id` (itself included) whose kind is one of `kinds`, in source order.
    fn within(&self, id: u32, kinds: &[&str]) -> Vec<u32> {
        let mut found = Vec::new();
        let mut stack = vec![id];
        while let Some(n) = stack.pop() {
            if kinds.contains(&self.kind(n)) {
                found.push(n);
            }
            stack.extend(self.tree.children(n).iter().map(|e| e.node));
        }
        found.sort_unstable_by_key(|&n| self.span(n)[0]);
        found
    }
    /// The `$Name` placeholders of a quote: each name and its span, `$` excluded.
    fn holes(&self, quote: u32) -> Vec<(&str, [usize; 2])> {
        let start = self.span(quote)[0];
        let text = self.text(quote);
        let bytes = text.as_bytes();
        let word = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
        let mut found = Vec::new();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] != b'$' {
                i += 1;
                continue;
            }
            let end = (i + 1..bytes.len())
                .find(|&j| !word(bytes[j]))
                .unwrap_or(bytes.len());
            if end > i + 1 && !bytes[i + 1].is_ascii_digit() {
                found.push((&text[i + 1..end], [start + i + 1, start + end]));
            }
            i = end;
        }
        found
    }
    /// A local bound by `binder`, a node with a `Name` field.
    fn local(&self, kind: &'static str, binder: u32) -> crate::GrammarLocal {
        crate::GrammarLocal {
            kind,
            name: self.named(binder, "Name"),
            detail: self
                .text(binder)
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            definitions: vec![self.span(self.field(binder, "Name"))],
            uses: vec![],
        }
    }
    /// A helper's parameters, used by its selector and quotes, and each
    /// arm's captures, which shadow parameters within the arm.
    fn helper_locals(&self, id: u32, found: &mut Vec<crate::GrammarLocal>) {
        let mut parameters: Vec<_> = self
            .fields(id, "Parameters")
            .into_iter()
            .map(|p| self.local("parameter", p))
            .collect();
        let body = self.first(self.field(id, "Body"));
        if self.kind(body) == "QuoteBody" {
            for (name, span) in self.holes(self.field(body, "Text")) {
                bind(&mut parameters, name, span);
            }
            found.extend(parameters);
            return;
        }
        let selector = self.field(body, "Selector");
        bind(&mut parameters, self.text(selector), self.span(selector));
        for arm in self.fields(body, "Arms") {
            let form = self.field(arm, "Form");
            let mut captures: Vec<_> = match self.tree.field(form, "Captures") {
                Some(c) => self
                    .fields(c, "Items")
                    .into_iter()
                    .map(|p| self.local("capture", p))
                    .collect(),
                None => vec![],
            };
            let quotes = match self.tree.field(form, "Pattern") {
                Some(pattern) => vec![pattern, self.field(form, "Result")],
                None => vec![self.field(form, "Result")],
            };
            for quote in quotes {
                for (name, span) in self.holes(quote) {
                    if !bind(&mut captures, name, span) {
                        bind(&mut parameters, name, span);
                    }
                }
            }
            found.extend(captures);
        }
        found.extend(parameters);
    }
    /// The element labels of a rule or alternative, used by its clauses:
    /// quote placeholders, helper and lowering arguments, and named fields.
    fn label_locals(&self, id: u32, found: &mut Vec<crate::GrammarLocal>) {
        let Some(body) = self.tree.field(id, "Body") else {
            return;
        };
        let mut labels: Vec<crate::GrammarLocal> = Vec::new();
        for label in self.within(body, &["Labelled"]) {
            let name = self.field(label, "Name");
            match labels.iter_mut().find(|l| l.name == self.text(name)) {
                Some(local) => local.definitions.push(self.span(name)),
                None => labels.push(self.local("field", label)),
            }
        }
        if labels.is_empty() {
            return;
        }
        const USING: [&str; 9] = [
            "QuoteText",
            "HelperCall",
            "Call",
            "Declares",
            "References",
            "LiteralLowering",
            "ValueLowering",
            "Template",
            "Expand",
        ];
        for clause in self.fields(id, "Clauses") {
            for n in self.within(clause, &USING) {
                let names = match self.kind(n) {
                    "QuoteText" => {
                        for (name, span) in self.holes(n) {
                            bind(&mut labels, name, span);
                        }
                        continue;
                    }
                    "HelperCall" | "Call" => self
                        .fields(n, "Args")
                        .into_iter()
                        .map(|a| match self.kind(a) {
                            "Argument" => self.field(self.first(a), "Name"),
                            _ => self.field(a, "Name"),
                        })
                        .collect(),
                    _ => vec![self.field(n, "Field")],
                };
                for name in names {
                    bind(&mut labels, self.text(name), self.span(name));
                }
            }
        }
        found.extend(labels);
    }
    fn literal(&self, id: u32) -> Result<String, PrepareError> {
        crate::literal::decode(self.source, self.span(id), self.kind(id) == "Char")
    }
    fn modules(&self, index: usize) -> Result<Vec<Module>, PrepareError> {
        self.fields(self.tree.root, "Modules")
            .into_iter()
            .map(|id| {
                let mut module = Module {
                    name: self.named(id, "Name"),
                    imports: vec![],
                    symbols: vec![],
                    builtins: vec![],
                    helpers: vec![],
                    functions: vec![],
                    rules: vec![],
                    extensions: vec![],
                    source: index,
                    span: self.span(id),
                };
                for member in self.fields(id, "Members") {
                    let m = self.first(member);
                    match self.kind(m) {
                        "Using" => module.imports.push(self.named(m, "Name")),
                        "SymbolProperty" => self.semantic_member(m)?,
                        "Symbols" => module.symbols.extend(
                            self.fields(m, "Names")
                                .iter()
                                .map(|&n| self.text(n).to_owned()),
                        ),
                        "Builtin" => {
                            let kind = self.named(m, "Kind");
                            module.builtins.extend(
                                self.fields(m, "Names")
                                    .iter()
                                    .map(|&n| (kind.clone(), self.text(n).into())),
                            );
                        }
                        "Helper" => module.helpers.push(self.helper(m)?),
                        "Function" => module.functions.push(self.function(m)?),
                        "Extend" => {
                            let alternatives = self
                                .fields(m, "Alternatives")
                                .into_iter()
                                .map(|n| self.rule(n))
                                .collect::<Result<_, _>>()?;
                            module
                                .extensions
                                .push((self.named(m, "Name"), alternatives));
                        }
                        _ => module.rules.push(self.rule(m)?),
                    }
                }
                Ok(module)
            })
            .collect()
    }
    /// The raw text between `<[` and the first `]>`, as Nitrogen's QuoteText holds it.
    fn quote(&self, id: u32) -> String {
        let text = self.text(id);
        text[2..text.len() - 2].into()
    }
    fn function(&self, id: u32) -> Result<Function, PrepareError> {
        let result = self.field(id, "Result");
        Ok(Function {
            name: self.named(id, "Name"),
            parameters: self.parameters(self.fields(id, "Parameters"))?,
            result: (self.text(result).into(), self.span(result)),
            lets: self.lets(id)?,
            body: self.expr(self.field(id, "Body"))?,
            span: self.span(id),
        })
    }
    fn lets(&self, id: u32) -> Result<Vec<(String, Value)>, PrepareError> {
        self.fields(id, "Lets")
            .into_iter()
            .map(|n| Ok((self.named(n, "Name"), self.value(self.field(n, "Value"))?)))
            .collect()
    }
    fn block(&self, id: u32) -> Result<Value, PrepareError> {
        let condition = self
            .tree
            .field(id, "Where")
            .map(|w| self.value(self.field(w, "Condition")))
            .transpose()?;
        Ok(
            json!({"lets":self.lets(id)?.into_iter().map(|(n, v)| json!([n, v])).collect::<Vec<_>>(),
            "where":condition,"body":self.expr(self.field(id, "Body"))?}),
        )
    }
    fn call_name(&self, id: u32) -> String {
        let head = self.text(self.field(id, "Head"));
        head[..head.len() - 1].into()
    }
    fn class_items(&self, id: u32) -> Result<Vec<Value>, PrepareError> {
        self.fields(id, "Ranges")
            .into_iter()
            .map(|n| {
                let last = self
                    .tree
                    .field(n, "High")
                    .map(|h| self.value(self.field(h, "Value")))
                    .transpose()?;
                Ok(json!({"first":self.value(self.field(n, "Low"))?,"last":last}))
            })
            .collect()
    }
    /// A value expression, as a JSON object tagged by `"v"`.
    fn value(&self, id: u32) -> Result<Value, PrepareError> {
        let binary = |op: &str, left: Value, right: Value| json!({"v":"binary","op":op,"left":left,"right":right});
        let fold = |op: &str| -> Result<Value, PrepareError> {
            let mut items = self.fields(id, "Items").into_iter();
            let mut result = self.value(items.next().unwrap())?;
            for item in items {
                result = binary(op, result, self.value(item)?);
            }
            Ok(result)
        };
        let mut value = match self.kind(id) {
            "Value" | "NotValue" | "UnaryValue" | "AtomValue" | "Bound" => {
                return self.value(self.first(id));
            }
            "GroupValue" => return self.value(self.field(id, "Inner")),
            "IfValue" => json!({"v":"if","cond":self.value(self.field(id,"Condition"))?,
                "then":self.value(self.field(id,"Then"))?,"else":self.value(self.field(id,"Else"))?}),
            "OrValue" => fold("or")?,
            "AndValue" => fold("and")?,
            "BitOrValue" => fold("|")?,
            "BitAndValue" => fold("&")?,
            "NotApplied" => {
                json!({"v":"unary","op":"not","operand":self.value(self.field(id,"Operand"))?})
            }
            "UnaryApplied" => {
                json!({"v":"unary","op":self.named(id,"Op"),"operand":self.value(self.field(id,"Operand"))?})
            }
            "CompareValue" => {
                let left = self.value(self.field(id, "Left"))?;
                match self.tree.field(id, "Tail") {
                    Some(t) => binary(
                        &self.named(t, "Op"),
                        left,
                        self.value(self.field(t, "Right"))?,
                    ),
                    None => return Ok(left),
                }
            }
            "AddValue" | "MulValue" => {
                let mut result = self.value(self.field(id, "First"))?;
                for tail in self.fields(id, "Rest") {
                    result = binary(
                        &self.named(tail, "Op"),
                        result,
                        self.value(self.field(tail, "Item"))?,
                    );
                }
                result
            }
            "PowValue" => {
                let base = self.value(self.field(id, "Base"))?;
                match self.tree.field(id, "Exponent") {
                    Some(t) => binary("^", base, self.value(self.field(t, "Value"))?),
                    None => return Ok(base),
                }
            }
            "IntValue" => {
                let digits = self.field(id, "Value");
                let n = self
                    .text(digits)
                    .parse::<i64>()
                    .map_err(|_| self.error(digits, "the integer is too large"))?;
                json!({"v":"int","n":n})
            }
            "CharValue" => {
                json!({"v":"char","c":crate::literal::decode_unit(self.source, self.span(self.field(id, "Value")))?})
            }
            // A bare character bound, as the previous seed's `Range` yields it while bootstrapping.
            "Char" => {
                json!({"v":"char","c":crate::literal::decode_unit(self.source, self.span(id))?})
            }
            "TextValue" => json!({"v":"text","s":self.literal(self.field(id, "Value"))?}),
            "ClassValue" => {
                json!({"v":"class","negated":self.tree.field(id,"Negated").is_some(),"items":self.class_items(id)?})
            }
            "BlockValue" => json!({"v":"block","block":self.block(self.field(id, "Body"))?}),
            "CallValue" => json!({"v":"call","name":self.call_name(id),
                "args":self.fields(id,"Args").into_iter().map(|a| self.value(a)).collect::<Result<Vec<_>,_>>()?}),
            "NameValue" => json!({"v":"name","name":self.named(id,"Name")}),
            other => unreachable!("generated value variant: {other}"),
        };
        value["_span"] = json!(self.span(id));
        Ok(value)
    }
    fn parameters(&self, ids: Vec<u32>) -> Result<Vec<Parameter>, PrepareError> {
        ids.into_iter()
            .map(|n| {
                if self.tree.field(n, "List").is_some() {
                    return Err(self.error(n, "list helper parameters are not admitted"));
                }
                Ok(Parameter {
                    name: self.named(n, "Name"),
                    category: self.named(n, "Category"),
                    span: self.span(n),
                })
            })
            .collect()
    }
    fn helper(&self, id: u32) -> Result<Helper, PrepareError> {
        let result = self.field(id, "Result");
        let mut helper = Helper {
            name: self.named(id, "Name"),
            parameters: self.parameters(self.fields(id, "Parameters"))?,
            result: (self.text(result).into(), self.span(result)),
            body: String::new(),
            selector: None,
            arms: vec![],
            span: self.span(id),
        };
        let body = self.first(self.field(id, "Body"));
        if self.kind(body) == "QuoteBody" {
            helper.body = self.quote(self.field(body, "Text"));
            return Ok(helper);
        }
        helper.selector = Some(self.named(body, "Selector"));
        let arms = self.fields(body, "Arms");
        for (index, arm) in arms.iter().enumerate() {
            let form = self.field(*arm, "Form");
            if self.kind(form) == "Fallback" {
                if index + 1 != arms.len() {
                    return Err(self.error(*arm, "the fallback must be the final arm"));
                }
                helper.body = self.quote(self.field(form, "Result"));
                return Ok(helper);
            }
            helper.arms.push(Arm {
                captures: match self.tree.field(form, "Captures") {
                    Some(c) => self.parameters(self.fields(c, "Items"))?,
                    None => vec![],
                },
                pattern: self.quote(self.field(form, "Pattern")),
                result: self.quote(self.field(form, "Result")),
            });
        }
        Err(self.error(body, "a match helper requires a final fallback"))
    }
    fn rule(&self, id: u32) -> Result<Rule, PrepareError> {
        let body = self
            .tree
            .field(id, "Body")
            .map(|n| self.expr(n))
            .transpose()?
            .unwrap_or(Value::Null);
        let name_node = self
            .tree
            .field(id, "Name")
            .or_else(|| self.tree.field(id, "Named").map(|n| self.field(n, "Name")));
        let name = match name_node {
            Some(n) => self.text(n).into(),
            None if body["kind"] == "ref" => body["name"]
                .as_str()
                .unwrap()
                .rsplit('.')
                .next()
                .unwrap()
                .into(),
            None => {
                return Err(self.error(id, "unnamed or transparent alternative is not admitted"));
            }
        };
        let mut rule = Rule {
            name,
            body,
            lexical: self.kind(id) == "TokenRule",
            clauses: vec![],
            block: false,
            except: vec![],
            precedence: None,
            associativity: None,
            alternatives: None,
            transparent: self.tree.field(id, "Transparent").is_some(),
            span: self.span(id),
        };
        if let Some(n) = name_node {
            rule.span[0] = self.span(n)[0];
        }
        if self.kind(id) == "ExtensibleRule" {
            for property in self.fields(id, "Properties") {
                self.semantic_member(property)?;
            }
            rule.alternatives = Some(
                self.fields(id, "Alternatives")
                    .into_iter()
                    .map(|n| self.rule(n))
                    .collect::<Result<_, _>>()?,
            );
        }
        if let Some(n) = self.tree.field(id, "Except") {
            for word in self.fields(n, "Words") {
                let text = self.literal(word)?;
                if text.is_empty() {
                    return Err(self.error(word, "empty excluded literal"));
                }
                rule.except.push(text);
            }
        }
        if let Some(n) = self.tree.field(id, "Precedence") {
            let level = self.field(n, "Level");
            let value = self
                .text(level)
                .parse::<u32>()
                .map_err(|_| self.error(level, "precedence too large"))?;
            if value > 255 {
                return Err(self.error(level, "precedence outside 0..255"));
            }
            rule.precedence = Some(value);
            rule.associativity = self.tree.field(n, "Associativity").map(|a| {
                if self.text(a) == "left" {
                    "Left".into()
                } else {
                    "Right".into()
                }
            });
        }
        for c in self.fields(id, "Clauses") {
            let clause = self.first(c);
            let computed = self.kind(clause) == "Lowers"
                && self.kind(self.field(clause, "Form")) == "SemanticLowering";
            if computed {
                self.semantic_member(c)?;
            } else {
                rule.clauses.push(self.clause(clause));
            }
        }
        let semantics = self.tree.field(id, "Semantics").or_else(|| {
            self.tree
                .field(id, "End")
                .map(|n| self.first(n))
                .filter(|&n| self.kind(n) == "Semantics")
        });
        rule.block = semantics.is_some();
        Ok(rule)
    }
    fn expr(&self, id: u32) -> Result<Value, PrepareError> {
        let mut value = match self.kind(id) {
            "Expression" | "Sequence" => {
                let items = self
                    .fields(id, "Items")
                    .into_iter()
                    .map(|n| self.expr(n))
                    .collect::<Result<Vec<_>, _>>()?;
                if items.len() == 1 {
                    return Ok(items.into_iter().next().unwrap());
                }
                json!({"kind":if self.kind(id) == "Expression" {"choice"} else {"sequence"},"items":items})
            }
            "Element" | "Unary" | "Primary" => return self.expr(self.first(id)),
            "Labelled" => {
                json!({"kind":"label","label":self.named(id,"Name"),"inner":self.expr(self.field(id,"Inner"))?})
            }
            "Predicate" => {
                json!({"kind":"predicate","positive":self.named(id,"Op")=="&","inner":self.expr(self.field(id,"Inner"))?})
            }
            "Postfixed" => {
                let inner = self.field(id, "Inner");
                // Primary is a structural wrapper around a concrete variant.
                let primary = self.first(inner);
                let mut ops = self.fields(id, "Ops").into_iter();
                let mut value = if self.kind(primary) == "Group"
                    && self.tree.field(primary, "Separator").is_some()
                {
                    let item = self.expr(self.field(primary, "Inner"))?;
                    let separator =
                        self.expr(self.field(self.field(primary, "Separator"), "Value"))?;
                    if separator["kind"] != "literal"
                        || separator["text"].as_str().is_none_or(|s| s.is_empty())
                    {
                        return Err(self.error(
                            primary,
                            "separated lists require a nonempty literal delimiter",
                        ));
                    }
                    let op = ops
                        .next()
                        .ok_or_else(|| self.error(primary, "separated list requires '*' or '+'"))?;
                    if !matches!(self.text(op), "*" | "+") {
                        return Err(self.error(op, "separated list requires '*' or '+'"));
                    }
                    let tail = json!({"kind":"repeat","min":0,"max":i32::MAX,"inner":{"kind":"sequence","items":[separator,item.clone()]}});
                    let body = json!({"kind":"sequence","items":[item,tail]});
                    if self.text(op) == "+" {
                        body
                    } else {
                        json!({"kind":"repeat","min":0,"max":1,"inner":body})
                    }
                } else {
                    self.expr(inner)?
                };
                for (depth, op) in ops.enumerate() {
                    if depth >= 128 {
                        return Err(self.error(op, "grammar nesting exceeds 128"));
                    }
                    value = json!({"kind":"repeat","min":if self.text(op)=="+"{1}else{0},"max":if self.text(op)=="?"{1}else{i32::MAX},"inner":value,"_span":self.span(id)});
                }
                return Ok(value);
            }
            "Literal" => {
                let text = self.literal(self.field(id, "Value"))?;
                if text.is_empty() {
                    return Err(self.error(id, "empty literal is not admitted"));
                }
                json!({"kind":"literal","text":text})
            }
            "Reference" => json!({"kind":"ref","name":self.named(id,"Name")}),
            "Any" => json!({"kind":"any"}),
            "Group" => return self.expr(self.field(id, "Inner")),
            "EmptyGroup" => json!({"kind":"sequence","items":[]}),
            "FunctionCall" => json!({"kind":"call","name":self.call_name(id),
                "args":self.fields(id,"Args").into_iter().map(|a| self.value(a)).collect::<Result<Vec<_>,_>>()?}),
            "If" => json!({"kind":"if","cond":self.value(self.field(id,"Condition"))?,
                "then":self.expr(self.field(id,"Then"))?,"else":self.expr(self.field(id,"Else"))?}),
            "Comprehension" => {
                let to = self
                    .tree
                    .field(id, "To")
                    .map(|t| self.value(self.field(t, "Value")))
                    .transpose()?;
                json!({"kind":"comprehension","choice":self.named(id,"Kind")=="choice","var":self.named(id,"Variable"),
                    "from":self.value(self.field(id,"From"))?,"to":to,"block":self.block(self.field(id,"Block"))?})
            }
            "Class" => {
                let items = self.class_items(id)?;
                let negated = self.tree.field(id, "Negated").is_some();
                let char_of =
                    |v: &Value| (v["v"] == "char").then(|| v["c"].as_u64().unwrap() as u32);
                if items.iter().all(|i| {
                    char_of(&i["first"]).is_some()
                        && (i["last"].is_null() || char_of(&i["last"]).is_some())
                }) {
                    let mut ranges = Vec::new();
                    for (item, n) in items.iter().zip(self.fields(id, "Ranges")) {
                        let low = char_of(&item["first"]).unwrap();
                        let high = if item["last"].is_null() {
                            low
                        } else {
                            char_of(&item["last"]).unwrap()
                        };
                        if high < low {
                            return Err(self.error(n, "empty character range"));
                        }
                        ranges.push([low, high]);
                    }
                    json!({"kind":"class","negated":negated,"ranges":ranges})
                } else {
                    json!({"kind":"valueclass","negated":negated,"items":items})
                }
            }
            _ => unreachable!("generated expression variant: {}", self.kind(id)),
        };
        value["_span"] = json!(self.span(id));
        Ok(value)
    }
    fn clause(&self, id: u32) -> Value {
        let mut field = String::new();
        let mut target = None;
        let mut args = Vec::new();
        let mut text_args = Vec::new();
        let mut template = None;
        let mut kinds = Vec::new();
        let mut sequential = false;
        let mut exported = false;
        let kind = match self.kind(id) {
            "Declares" => {
                field = self.named(id, "Field");
                kinds.push(self.named(id, "Kind"));
                sequential = self.tree.field(id, "Sequential").is_some();
                exported = self.tree.field(id, "Export").is_some();
                target = self.tree.field(id, "Type").map(|n| self.named(n, "Name"));
                "Declares"
            }
            "References" => {
                field = self.named(id, "Field");
                let n = self.first(self.field(id, "Kinds"));
                if self.kind(n) == "SingleKind" {
                    kinds.push(self.named(n, "Name"));
                } else {
                    kinds.extend(
                        self.fields(n, "Names")
                            .iter()
                            .map(|&w| self.named(w, "Name")),
                    );
                }
                "References"
            }
            "Scope" => "Scope",
            "Rewrites" => {
                let n = self.field(id, "Form");
                if self.kind(n) == "QuoteBody" {
                    template = Some(self.quote(self.field(n, "Text")));
                } else {
                    target = Some(self.named(n, "Target"));
                    args.extend(
                        self.fields(n, "Args")
                            .iter()
                            .map(|&w| self.named(w, "Name")),
                    );
                }
                "Rewrites"
            }
            "Lowers" => {
                let n = self.field(id, "Form");
                match self.kind(n) {
                    "Template" | "Expand" => {
                        field = self.named(n, "Field");
                        args.push(self.named(n, "Args"));
                        if self.kind(n) == "Template" {
                            "LowersTemplate"
                        } else {
                            "LowersExpand"
                        }
                    }
                    "LiteralLowering" => {
                        target = Some(self.named(n, "Target"));
                        field = self.named(n, "Field");
                        "LowersLiteral"
                    }
                    "Call" => {
                        target = Some(self.named(n, "Target"));
                        for (index, argument) in self.fields(n, "Args").into_iter().enumerate() {
                            let form = self.first(argument);
                            if self.kind(form) == "TextArgument" {
                                text_args.push(index);
                            }
                            args.push(self.named(form, "Name"));
                        }
                        "Lowers"
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        };
        let mut value = json!({"kind":kind,"field":field,"target":target,"args":args,"symbol_kinds":kinds,"sequential":sequential});
        if exported {
            value["export"] = json!(true);
        }
        if !text_args.is_empty() {
            value["text_args"] = json!(text_args);
        }
        if let Some(template) = template {
            value["template"] = json!(template);
        }
        value
    }
}
