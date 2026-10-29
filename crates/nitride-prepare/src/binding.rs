//! Binding files (`.nbind`): parsed by the generated binding frontend and
//! checked against the project's lowered catalog.
use crate::{PrepareError, Source};
use nitride_core::{Limits, SyntaxTree};
use serde_json::{Map, Value, json};

/// The binding frontend's tree of one source.
fn syntax(source: &Source) -> Result<SyntaxTree<'static>, PrepareError> {
    nitride_core::parse_with_options(
        &crate::binding_seed::LANGUAGE,
        &source.text,
        Limits::default(),
        nitride_core::ParseOptions {
            strict_comments: true,
            token_children: false,
            alias_nodes: true,
        },
    )
    .map_err(|e| source.error(e.span.map(|v| v as usize), e.message))
}

/// A name as written, where it was written.
struct Name {
    text: String,
    span: [usize; 2],
}

struct Decoder<'a> {
    tree: &'a SyntaxTree<'static>,
}

impl Decoder<'_> {
    fn kind(&self, id: u32) -> &str {
        crate::binding_seed::LANGUAGE.rules[self.tree.nodes[id as usize].rule as usize]
            .name
            .rsplit('.')
            .next()
            .unwrap()
    }
    fn span(&self, id: u32) -> [usize; 2] {
        self.tree.nodes[id as usize].span.map(|v| v as usize)
    }
    /// The node of one of `kinds` that `id` stands for (choices wrap it).
    fn concrete(&self, mut id: u32, kinds: &[&str]) -> u32 {
        while !kinds.contains(&self.kind(id)) {
            id = self.tree.children(id)[0].node;
        }
        id
    }
    fn node(&self, id: u32, label: &str) -> u32 {
        self.tree.field(id, label).expect("generated binding field")
    }
    fn nodes(&self, id: u32, label: &str) -> Vec<u32> {
        self.tree
            .children(id)
            .iter()
            .filter(|e| self.tree.label(e) == label)
            .map(|e| e.node)
            .collect()
    }
    fn name(&self, id: u32, label: &str) -> Name {
        let id = self.node(id, label);
        Name {
            text: self.tree.text(id).unwrap().into(),
            span: self.span(id),
        }
    }
}

/// The bindings of a project's `.nbind` sources, checked against its lowered
/// catalog: per binding, the interface, its contract, the target, and the
/// export name of every interface operation. A component path is as written,
/// relative to its binding file.
pub fn lower(sources: &[Source], catalog: &Value) -> Result<Value, PrepareError> {
    let mut bound: Map<String, Value> = Map::new();
    let mut lowered = Vec::new();
    for source in sources {
        let tree = syntax(source)?;
        let d = Decoder { tree: &tree };
        for binding in d.nodes(tree.root, "Bindings") {
            let interface = d.name(binding, "Interface");
            let Some(declared) = catalog["interfaces"].get(&interface.text) else {
                return Err(source.error(
                    interface.span,
                    format!("unknown interface '{}'", interface.text),
                ));
            };
            if let Some(other) = bound.get(&interface.text) {
                return Err(source.error(
                    interface.span,
                    format!(
                        "'{}' is already bound in {}",
                        interface.text,
                        other.as_str().unwrap()
                    ),
                ));
            }
            bound.insert(interface.text.clone(), json!(source.path));
            let version = d.name(binding, "Version");
            if version.text.parse::<u64>().ok() != declared["version"].as_u64() {
                return Err(source.error(
                    version.span,
                    format!(
                        "'{}' is version {} in the catalog",
                        interface.text, declared["version"]
                    ),
                ));
            }
            let target = d.concrete(d.node(binding, "Target"), &["WasmComponent"]);
            let path = d.node(target, "Path");
            let component = crate::literal::decode(source, d.span(path), false)?;
            if component.is_empty() {
                return Err(source.error(d.span(path), "a component path is not empty"));
            }
            // Every operation is exported under its WIT name unless renamed.
            let module = interface.text.rsplit_once('.').map_or("", |(m, _)| m);
            let operations: Vec<&str> = declared["operations"]
                .as_array()
                .unwrap()
                .iter()
                .map(|o| o.as_str().unwrap())
                .collect();
            let mut exports = Map::new();
            for op in &operations {
                let tail = op.rsplit('.').next().unwrap();
                exports.insert((*op).into(), json!(nitride_codegen::wit_name(tail)));
            }
            let body = d.concrete(d.node(binding, "Body"), &["Exports", "End"]);
            let mut renamed: Vec<String> = Vec::new();
            for export in d.nodes(body, "Members") {
                let export = d.concrete(export, &["Export"]);
                let operation = d.name(export, "Operation");
                let id = format!("{module}.{}", operation.text);
                if !operations.contains(&id.as_str()) {
                    return Err(source.error(
                        operation.span,
                        format!(
                            "'{}' is not an operation of '{}'",
                            operation.text, interface.text
                        ),
                    ));
                }
                if renamed.contains(&id) {
                    return Err(source.error(
                        operation.span,
                        format!("'{}' is already renamed", operation.text),
                    ));
                }
                renamed.push(id.clone());
                exports.insert(id, json!(d.name(export, "Name").text));
            }
            lowered.push(json!({
                "interface": interface.text,
                "contract": format!("{}@{}", interface.text, declared["hash"].as_str().unwrap()),
                "target": "wasm-component",
                "component": component,
                "exports": exports,
            }));
        }
    }
    Ok(Value::Array(lowered))
}

/// Whether a binding file parses as written.
pub fn parses(source: &Source) -> bool {
    syntax(source).is_ok()
}
