using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using System.Text.Json.Nodes;
using Nitrogen.Grammar;

namespace Nitride.Reference;

static class Exporter
{
    public static string Export(string source, string catalog) => ExportSources([source], catalog, null, "typed");

    public static string ExportProject(string manifestPath)
    {
        var manifest = JsonNode.Parse(File.ReadAllText(manifestPath))!;
        var directory = Path.GetDirectoryName(Path.GetFullPath(manifestPath))!;
        var files = manifest["grammars"]!.AsArray().Select(p => File.ReadAllText(Path.Combine(directory, (string)p!))).ToArray();
        string catalog = File.ReadAllText(Path.Combine(directory, (string)manifest["catalog"]!));
        return ExportSources(files, catalog, (string?)manifest["start"], (string?)manifest["mode"] ?? "typed");
    }

    static string ExportSources(string[] sources, string catalog, string? requestedStart, string mode)
    {
        if (sources.Length == 0 || mode is not ("typed" or "syntax")) throw Unsupported("empty input or unknown admission mode");
        bool syntaxOnly = mode == "syntax";
        var modules = new List<ModuleDecl>();
        foreach (string source in sources)
        {
            var parsed = GrammarParser.Parse(source);
            if (!parsed.Success) throw Unsupported(string.Join("; ", parsed.Diagnostics.Select(d => d.Message)));
            if (parsed.File!.Languages.Count != 0) throw Unsupported("language declarations belong in the manifest");
            modules.AddRange(parsed.File.Modules);
        }
        var inputs = sources.Select((source, i) => new GrammarInput($"input{i}.ngr", source, "Fixture")).ToArray();
        var compiled = syntaxOnly ? GrammarCompiler.CompileQuotationSyntax(inputs) : GrammarCompiler.Compile(inputs);
        if (compiled.HasErrors) throw Unsupported(string.Join("; ", compiled.Diagnostics.Select(d => d.Diagnostic.Message)));
        var symbols = modules.SelectMany(m => m.Rules.Select(r => (Key: m.Name + "." + r.Name, Module: m, Rule: r))).ToDictionary(r => r.Key);
        string Resolve(string name, ModuleDecl from, string? implicitModule = null)
        {
            if (name.Contains('.')) return symbols.ContainsKey(name) ? name : throw Unsupported("unknown rule " + name);
            string local = from.Name + "." + name;
            if (symbols.ContainsKey(local)) return local;
            var imported = from.Usings.Select(u => u.Module).Concat(implicitModule is null ? [] : new[] { implicitModule })
                .Where(m => m != from.Name).Distinct(StringComparer.Ordinal)
                .Select(m => m + "." + name).Where(symbols.ContainsKey).ToArray();
            if (imported.Length != 1) throw Unsupported("unknown or ambiguous rule " + name);
            return imported[0];
        }
        var points = modules.SelectMany(m => m.Rules.OfType<ExtensibleRule>().Select(p => (Key: m.Name + "." + p.Name, Module: m, Rule: p)))
            .ToDictionary(p => p.Key, p => p.Rule.Alternatives.Select(a => (Alternative: a, Module: p.Module, Implicit: p.Module.Name)).ToList());
        foreach (var module in modules)
        {
            if (module.SymbolProperties.Count + module.Helpers.Count + module.Functions.Count != 0) throw Unsupported("properties or helpers");
            foreach (var extension in module.Extends)
            {
                string target = Resolve(extension.Target, module);
                if (!points.TryGetValue(target, out var alternatives)) throw Unsupported("extension target is not extensible: " + target);
                var owner = symbols[target].Module.Name;
                alternatives.AddRange(extension.Alternatives.Select(a => (a, module, owner)));
            }
        }
        var builtins = new JsonArray();
        var builtinKeys = new HashSet<(string, string)>();
        foreach (var module in modules)
            foreach (var declaration in module.Builtins)
            {
                if (declaration.Scope is not null) throw Unsupported("scoped builtins");
                foreach (var name in declaration.Names)
                {
                    if (!builtinKeys.Add((declaration.Kind.Name, name.Name))) throw Unsupported("duplicate builtin");
                    builtins.Add(new JsonObject { ["kind"] = declaration.Kind.Name, ["name"] = name.Name });
                }
            }
        var rules = new JsonArray();
        var omitted = new JsonArray();
        void NoteOmitted(string name, SemanticsBlock? block, EquatableArray<BindingClause> clauses)
        {
            if (block != null || clauses.Count > 0)
                omitted.Add(new JsonObject { ["rule"] = name, ["block"] = block != null,
                    ["clauses"] = JsonSerializer.SerializeToNode(clauses.Select(c => c.Kind.ToString())) });
        }
        JsonNode Expression(Expr expression, ModuleDecl module, string? implicitModule) => expression switch
        {
            LiteralExpr e => Obj("literal", ("text", JsonValue.Create(e.Value))),
            ReferenceExpr e => Obj("ref", ("name", JsonValue.Create(Resolve(e.Name, module, implicitModule)))),
            CharClassExpr e => Obj("class", ("negated", JsonValue.Create(e.Negated)), ("ranges", JsonSerializer.SerializeToNode(e.Ranges.Select(r => new[] { (int)r.First, (int)r.Last })))),
            AnyCharExpr => Obj("any"),
            PredicateExpr e => Obj("predicate", ("positive", JsonValue.Create(e.Kind == PredicateKind.And)), ("inner", Expression(e.Inner, module, implicitModule))),
            SequenceExpr e => Obj("sequence", ("items", new JsonArray(e.Items.Select(i => Expression(i, module, implicitModule)).ToArray()))),
            ChoiceExpr e => Obj("choice", ("items", new JsonArray(e.Alternatives.Select(i => Expression(i, module, implicitModule)).ToArray()))),
            LabeledExpr e => Obj("label", ("label", JsonValue.Create(e.Label)), ("inner", Expression(e.Inner, module, implicitModule))),
            SeparatedListExpr e when e.Separator is LiteralExpr { Value.Length: > 0 } => Separated(e, module, implicitModule),
            RepeatExpr e => Obj("repeat", ("min", JsonValue.Create(e.Kind == RepeatKind.OneOrMore ? 1 : 0)),
                ("max", JsonValue.Create(e.Kind == RepeatKind.Optional ? 1 : int.MaxValue)), ("inner", Expression(e.Inner, module, implicitModule))),
            _ => throw Unsupported($"grammar expression {expression.GetType().Name} at {expression.Span.Start}")
        };
        JsonNode Separated(SeparatedListExpr e, ModuleDecl module, string? implicitModule)
        {
            var item=Expression(e.Item,module,implicitModule);
            var tail=Obj("repeat",("min",JsonValue.Create(0)),("max",JsonValue.Create(int.MaxValue)),("inner",Obj("sequence",("items",new JsonArray(Expression(e.Separator,module,implicitModule),item.DeepClone())))));
            var body=Obj("sequence",("items",new JsonArray(item,tail)));
            return e.AtLeastOne ? body : Obj("repeat",("min",JsonValue.Create(0)),("max",JsonValue.Create(1)),("inner",body));
        }
        JsonArray Clauses(EquatableArray<BindingClause> clauses)
        {
            var result = new JsonArray();
            if (syntaxOnly) return result;
            foreach (var c in clauses)
            {
                if (c.Kind is not (BindingClauseKind.Declares or BindingClauseKind.References or BindingClauseKind.Lowers or BindingClauseKind.LowersLiteral or BindingClauseKind.Scope or BindingClauseKind.LowersTemplate or BindingClauseKind.LowersExpand) ||
                    c.Optional || c.FileScope || c.Qualifier != null || c.TypeProperty != null || c.OperationProperty != null || c.InitializerProperty != null ||
                    c.Arguments.Any(a => a.SequenceElementType != null || a.InferSequence || (a.AsText && c.Kind != BindingClauseKind.Lowers) || a.OptionalElementType != null || a.OptionalText))
                    throw Unsupported($"semantic clause {c.Kind} at {c.Span.Start}");

                var clause = new JsonObject { ["kind"] = c.Kind.ToString(), ["field"] = c.Field, ["target"] = c.Target?.Name,
                    ["args"] = JsonSerializer.SerializeToNode(c.Arguments.Select(a => a.Name)),
                    ["symbol_kinds"] = JsonSerializer.SerializeToNode(c.Kinds.Select(k => k.Name)), ["sequential"] = c.Sequential };
                if (c.Export) clause["export"] = true;
                var texts = c.Arguments.Select((a, i) => (a, i)).Where(p => p.a.AsText).Select(p => p.i).ToArray();
                if (texts.Length > 0) clause["text_args"] = JsonSerializer.SerializeToNode(texts);
                result.Add(clause);
            }
            return result;
        }
        void Add(ModuleDecl module, string name, Expr body, bool lexical, EquatableArray<BindingClause> clauses = default,
            SemanticsBlock? semantics = null, string[]? except = null, int? precedence = null, string? owner = null, string? implicitModule = null, string? associativity = null)
        {
            string qualified = module.Name + "." + name;
            if (!syntaxOnly && semantics != null) throw Unsupported("C# semantics in " + qualified);
            if (syntaxOnly) NoteOmitted(qualified, semantics, clauses);
            rules.Add(new JsonObject { ["name"] = qualified, ["lexical"] = lexical, ["expr"] = Expression(body, module, implicitModule),
                ["clauses"] = Clauses(clauses), ["except"] = JsonSerializer.SerializeToNode(except ?? []),
                ["precedence"] = precedence, ["owner"] = owner, ["associativity"] = associativity });
        }
        foreach (var module in modules)
        foreach (var rule in module.Rules)
        {
            switch (rule)
            {
                case TokenRule t: Add(module, t.Name, t.Body, true, except: t.Except.Select(e => e.Value).ToArray()); break;
                case SyntaxRule s: Add(module, s.Name, s.Body, false, s.Clauses, s.Semantics); break;
                case ExtensibleRule e when e.Properties.Count == 0:
                    string point = module.Name + "." + e.Name;
                    var prefixes = new JsonArray(); var infixes = new JsonArray();
                    var operators = new HashSet<string>(StringComparer.Ordinal);
                    foreach (var p in points[point])
                    {
                        var a = p.Alternative;
                        if (a.IsTransparent || string.IsNullOrEmpty(a.Name)) throw Unsupported("transparent or unnamed alternative");
                        bool infix = First(a.Body) is ReferenceExpr r && Resolve(r.Name, p.Module, p.Implicit) == point;
                        if (infix && a.Precedence is null) throw Unsupported("infix precedence required");
                        if (infix)
                        {
                            var items = a.Body is SequenceExpr sequence ? sequence.Items.ToArray() : [];
                            Expr? suffix = items.Length > 1 ? items[1] : null;
                            if (suffix is LabeledExpr label) suffix = label.Inner;
                            if (suffix is not LiteralExpr literal) throw Unsupported("infix must start with a literal operator");
                            if (!operators.Add(literal.Value)) throw Unsupported("conflicting extension operator " + literal.Value + " for " + point);
                        }
                        Add(p.Module, a.Name, a.Body, false, a.Clauses, a.Semantics, precedence: a.Precedence, owner: point,
                            implicitModule: p.Implicit, associativity: infix ? (a.Associativity ?? GrammarAssociativity.Left).ToString() : null);
                        (infix ? infixes : prefixes).Add(p.Module.Name + "." + a.Name);
                    }
                    rules.Add(new JsonObject { ["name"] = point, ["lexical"] = false,
                        ["expr"] = Obj("pratt", ("prefixes", prefixes), ("infixes", infixes)), ["clauses"] = new JsonArray(), ["except"] = new JsonArray() });
                    break;
                default: throw Unsupported($"rule {rule.Name}: properties or unsupported rule form");
            }
        }
        string start = requestedStart ?? symbols.FirstOrDefault(s => s.Value.Rule is SyntaxRule && s.Value.Rule.Name == "Program").Key
            ?? symbols.First(s => s.Value.Rule is SyntaxRule).Key;
        if (!symbols.TryGetValue(start, out var startRule) || startRule.Rule is not (SyntaxRule or ExtensibleRule)) throw Unsupported("unknown syntax start rule " + start);
        var byName = rules.Cast<JsonObject>().ToDictionary(r => (string)r["name"]!);
        // The Rust preparation gate performs complete nullable/left-edge cycle checks.
        // Retain the immediate-cycle rejection in this exporter for actionable frontend errors.
        IEnumerable<string> Leading(JsonNode e) => (string)e["kind"]! switch
        {
            "ref" => [(string)e["name"]!], "label" or "repeat" or "predicate" => Leading(e["inner"]!),
            "sequence" => e["items"]!.AsArray().Count == 0 ? [] : Leading(e["items"]![0]!),
            "choice" => e["items"]!.AsArray().SelectMany(x => Leading(x!)),
            "pratt" => e["prefixes"]!.AsArray().Select(x => (string)x!), _ => []
        };
        void Visit(string name, HashSet<string> stack)
        {
            if (!byName.ContainsKey(name)) throw Unsupported("unknown rule " + name);
            if (!stack.Add(name)) throw Unsupported("left recursion " + name);
            foreach (var next in Leading(byName[name]["expr"]!)) Visit(next, stack);
            stack.Remove(name);
        }
        foreach (var name in byName.Keys) Visit(name, []);
        string identity = sources.Length == 1 ? sources[0] : JsonSerializer.Serialize(sources);
        var output = new JsonObject { ["version"] = 1, ["symbol_kinds"] = JsonSerializer.SerializeToNode(modules.SelectMany(m => m.Symbols).SelectMany(s => s.Kinds).Select(k => k.Name).Distinct(StringComparer.Ordinal)), ["builtins"] = builtins, ["mode"] = mode, ["omitted_semantics"] = omitted,
            ["modules"] = JsonSerializer.SerializeToNode(modules.Select(m => new { name = m.Name, imports = m.Usings.Select(u => u.Module).ToArray() })),
            ["source_hash"] = Hash(identity), ["catalog_hash"] = Hash(catalog),
            ["start"] = start, ["rules"] = rules, ["catalog"] = JsonNode.Parse(catalog) };
        return output.ToJsonString(new JsonSerializerOptions { WriteIndented = true });
    }
    static string Hash(string source) => Convert.ToHexString(SHA256.HashData(Encoding.UTF8.GetBytes(source))).ToLowerInvariant();
    static Expr First(Expr e) => e switch { LabeledExpr l => First(l.Inner), SequenceExpr s => First(s.Items[0]), _ => e };
    static JsonObject Obj(string kind, params (string, JsonNode?)[] fields)
    {
        var result = new JsonObject { ["kind"] = kind };
        foreach (var (key, value) in fields) result[key] = value;
        return result;
    }
    static Exception Unsupported(string text) => new ArgumentException("unsupported: " + text);
}
