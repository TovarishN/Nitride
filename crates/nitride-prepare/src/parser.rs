use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree};
use serde_json::{Value, json};

pub struct Module {
    pub name: String,
    pub imports: Vec<String>,
    pub symbols: Vec<String>,
    pub builtins: Vec<(String, String)>,
    pub helpers: Vec<Helper>,
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
pub fn parse(source: &Source, index: usize) -> Result<Vec<Module>, PrepareError> {
    let tree = nitride_core::parse_with_options(
        &crate::grammar_seed::LANGUAGE,
        &source.text,
        Limits {
            depth: 512,
            ..Limits::default()
        },
        nitride_core::ParseOptions {
            strict_comments: true,
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
    })?;
    let decoder = Decoder {
        source,
        tree: &tree,
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
            children.iter().filter(|e| e.label == "Ops").count()
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
    decoder.modules(index)
}

struct Decoder<'a> {
    source: &'a Source,
    tree: &'a SyntaxTree,
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
    fn field(&self, id: u32, label: &str) -> u32 {
        self.tree.field(id, label).expect("generated grammar field")
    }
    fn fields(&self, id: u32, label: &str) -> Vec<u32> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| e.label == label)
            .map(|e| e.node)
            .collect()
    }
    fn first(&self, id: u32) -> u32 {
        self.tree.children(id)[0].node
    }
    fn named(&self, id: u32, label: &str) -> String {
        self.text(self.field(id, label)).into()
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
                    rules: vec![],
                    extensions: vec![],
                    source: index,
                    span: self.span(id),
                };
                for member in self.fields(id, "Members") {
                    let m = self.first(member);
                    match self.kind(m) {
                        "Using" => module.imports.push(self.named(m, "Name")),
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
            rule.clauses.push(self.clause(self.first(c)));
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
            "Class" => {
                let mut ranges = Vec::new();
                for n in self.fields(id, "Ranges") {
                    let unit = |n| crate::literal::decode_unit(self.source, self.span(n));
                    let low = unit(self.field(n, "Low"))? as u32;
                    let high = match self.tree.field(n, "High") {
                        Some(h) => unit(self.field(h, "Value"))? as u32,
                        None => low,
                    };
                    if high < low {
                        return Err(self.error(n, "empty character range"));
                    }
                    ranges.push([low, high]);
                }
                json!({"kind":"class","negated":self.tree.field(id,"Negated").is_some(),"ranges":ranges})
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
