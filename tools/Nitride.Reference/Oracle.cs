using System.Text;
using System.Text.Json.Nodes;
using Nitrogen;
using Nitrogen.Binding;
using Nitrogen.Semantic;
using Nitrogen.Semantics;
using Nitride.Reference.Syntax;

namespace Nitride.Reference;

static class Oracle
{
    public static object Observe(string source, string catalogText, bool includePower = false, bool namespaces = false, bool inference = false, bool calls = false)
    {
        var data = JsonNode.Parse(catalogText)!;
        var types = data["types"]!.AsArray().Select(t => (string)t!).ToDictionary(t => t,
            t => SemanticType.Named(t[..t.LastIndexOf('.')], t[(t.LastIndexOf('.') + 1)..]));
        var operations = data["operations"]!.AsArray().Select(o => new OperationSignature((string)o!["id"]!, types[(string)o["result"]!],
            o["inputs"]!.AsArray().Select(t => types[(string)t!]).ToArray())).ToArray();
        var builder = new LanguageBuilder().Add(calls ? Nitride.Reference.Calls.CallsModule.Instance : inference ? Nitride.Reference.Inference.InferenceModule.Instance : namespaces ? Nitride.Reference.Namespaces.NamespacesModule.Instance : PortableModule.Instance);
        if (includePower) builder.Add(PowerModule.Instance);
        var language = builder.AddSemantic(new SemanticModule("Portable", [], types.Values, operations)).Build();
        using var parsed = language.Parse(source, calls ? Nitride.Reference.Calls.CallsModule.Program : inference ? Nitride.Reference.Inference.InferenceModule.Program : namespaces ? Nitride.Reference.Namespaces.NamespacesModule.Program : PortableModule.Program);
        int[] Span(TextSpan s) => [Encoding.UTF8.GetByteCount(source.AsSpan(0, s.Start)), Encoding.UTF8.GetByteCount(source.AsSpan(0, s.End))];
        object Error(string category, TextSpan span, string message) => new { ok = false, error = category, span = Span(span), message, trace = Array.Empty<string>() };
        if (!parsed.Success) return Error("syntax", parsed.Diagnostics[0].Span, "invalid source");
        var tree = parsed.Tree;
        var project = new Project(language);
        var binding = project.Set("input.portable", tree);
        var bindErrors = project.Diagnostics("input.portable");
        if (bindErrors.Count > 0) return Error("binding", bindErrors[0].Span, bindErrors[0].Message);
        using var semantics = new ProjectSemantics(project);
        var file = semantics["input.portable"];
        var typeErrors = file.Diagnostics();
        if (inference)
        {
            // Initializer consistency is this fixture adapter's explicit contract,
            // not declarative Nitrogen inference. Combine actual Nitrogen facts
            // and this added check under the documented source-order policy.
            var diagnostics = typeErrors.Select(d => (d.Span, d.Message)).ToList();
            foreach (var symbol in binding.Declarations)
            {
                int kind = tree.Kind(symbol.Node);
                int value = kind == Nitride.Reference.Inference.InferenceKinds.LetValue
                    ? new Nitride.Reference.Inference.LetValueNodeSemantics(file, symbol.Node).Value.Node
                    : kind == Nitride.Reference.Inference.InferenceKinds.LetRate
                        ? new Nitride.Reference.Inference.LetRateNodeSemantics(file, symbol.Node).Value.Node : -1;
                if (value < 0) continue;
                var declared = new Nitride.Reference.Inference.InferenceSymbol(file, symbol).Type;
                var context = new LoweringContext(file, Guid.NewGuid(), admission: LoweringAdmission.SyntaxOnly);
                var root = HirLowering.LowerNested(context, value);
                if (declared is not null && root is not null && !Equals(declared, root.Type))
                    diagnostics.Add((root.Origins[0].Span, "initializer differs from declared type"));
            }
            if (diagnostics.OrderBy(d => d.Span.Start).FirstOrDefault() is var diagnostic && diagnostic.Message is not null)
                return Error("type", diagnostic.Span, diagnostic.Message);
        }
        else if (typeErrors.Count > 0) return Error("type", typeErrors[0].Span, typeErrors[0].Message);
        var kinds = calls ? new HashSet<int> { Nitride.Reference.Calls.CallsKinds.Let, Nitride.Reference.Calls.CallsKinds.Local, Nitride.Reference.Calls.CallsKinds.Show } : inference ? new HashSet<int> { Nitride.Reference.Inference.InferenceKinds.LetValue, Nitride.Reference.Inference.InferenceKinds.LetRate, Nitride.Reference.Inference.InferenceKinds.InferValue, Nitride.Reference.Inference.InferenceKinds.InferRate, Nitride.Reference.Inference.InferenceKinds.Show } : namespaces ? new HashSet<int> { Nitride.Reference.Namespaces.NamespacesKinds.LetValue, Nitride.Reference.Namespaces.NamespacesKinds.LetRate, Nitride.Reference.Namespaces.NamespacesKinds.Show }
            : new HashSet<int> { PortableKinds.Let, PortableKinds.Show };
        var lowered = HirLowering.LowerSelected(file, kinds, Guid.NewGuid());
        if (lowered.Diagnostics.Count > 0) return Error("type", lowered.Diagnostics[0].Origin.Span, lowered.Diagnostics[0].Message);
        var statements = Enumerable.Range(0, tree.NodeCount).Where(n => kinds.Contains(tree.Kind(n))).ToArray();
        if (statements.Length != lowered.Roots.Count) throw new Exception("reference lowering did not produce one root per statement");
        var declarations = binding.Declarations.ToDictionary(s => s.Node);
        for (int i = 0; i < statements.Length; i++)
            if (declarations.TryGetValue(statements[i], out var symbol) && !(calls ? Equals(new Nitride.Reference.Calls.CallsSymbol(file, symbol).Type, lowered.Roots[i].Type) : inference ? Equals(new Nitride.Reference.Inference.InferenceSymbol(file, symbol).Type, lowered.Roots[i].Type) : Equals(file.DeclarativeTypes.TypeOfSymbol(symbol), lowered.Roots[i].Type)))
                return Error("type", lowered.Roots[i].Origins[0].Span, "initializer differs from declared type");
        var signatureMap = operations.ToDictionary(o => o.Id);
        // Generic HirEvaluator accepts Scalar/Angle only. Execute nominal Time through an
        // explicit fixture host over real Nitrogen HIR; never erase its nominal identity.
        _ = HostOperationRegistry.Bind(language.SemanticCatalog, operations.Select(o => new HostOperationBinding(o,
            (Func<IReadOnlyList<ExecutionValue>, ExecutionValue>)(a => new ExecutionValue(o.Result, a.Count == 0 ? 0 : a[0].Number)))));
        foreach (var node in lowered.Roots.SelectMany(HirTraversal.PreOrder))
            if (node is HirOperation op && (!signatureMap.TryGetValue(op.Signature.Id, out var expected) || !expected.Equals(op.Signature)))
                return Error("host", node.Origins[0].Span, "signature mismatch");
        var symbols = new Dictionary<Symbol, double>();
        var trace = new List<string>();
        var values = new List<double>();
        double Eval(HirNode node)
        {
            double value;
            if (node is HirConstant c) value = c.Value;
            else if (node is HirSymbolRef r) value = symbols[r.Symbol.Binding];
            else if (node is HirOperation op)
            {
                var args = op.Arguments.Select(Eval).ToArray();
                trace.Add(op.Signature.Id);
                string handler = (string)data["operations"]!.AsArray().Single(o => (string)o!["id"]! == op.Signature.Id)!["handler"]!;
                value = handler switch { "zero" => 0, "identity" => args[0], "neg" => -args[0], "add" => args[0] + args[1], "sub" => args[0] - args[1], "mul" => args[0] * args[1], "div" => args[0] / args[1], "pow" => Math.Pow(args[0], args[1]), _ => throw new Exception("unknown handler") };
            }
            else throw new Exception("unsupported HIR " + node.GetType().Name);
            if (!double.IsFinite(value)) throw new ArithmeticException("nonfinite result");
            return value;
        }
        object Hir(HirNode n) => n switch
        {
            HirConstant c => new { kind = "constant", type = n.Type.Id, span = Span(n.Origins[0].Span), number = (double)c.Value },
            HirSymbolRef r => new { kind = "reference", type = n.Type.Id, span = Span(n.Origins[0].Span), symbol = Span(r.Symbol.Binding.NameSpan)[0] },
            HirOperation o => new { kind = "operation", type = n.Type.Id, span = Span(n.Origins[0].Span), operation = o.Signature.Id, args = o.Arguments.Select(Hir).ToArray() },
            _ => throw new Exception("unsupported HIR")
        };
        try
        {
            for (int i = 0; i < statements.Length; i++)
            {
                double result = Eval(lowered.Roots[i]);
                if (declarations.TryGetValue(statements[i], out var symbol)) symbols[symbol] = result;
                else values.Add(result);
            }
        }
        catch (ArithmeticException e) { return new { ok = false, error = "execution", message = e.Message, trace }; }
        var semanticKinds = new HashSet<string> { "Let", "Show", "Num", "Ref", "Paren", "Duration", "Neg", "Add", "Sub", "Mul", "Div", "Pow", "LetValue", "LetRate", "ValueRef", "RateRef", "AnyRef", "ReverseRef", "InferValue", "InferRate", "Local", "Call0", "Call1", "Call2", "AnyCall", "FallbackCall" };
        return new { ok = true,
            syntax = Enumerable.Range(0, tree.NodeCount).Where(n => semanticKinds.Contains(language.GetKindName(tree.Kind(n))))
                .Select(n => new { kind = (calls ? "Calls." : inference ? "Inference." : namespaces ? "Namespaces." : includePower && tree.Kind(n) >> 16 == PowerModule.Instance.Id ? "Portable.Power." : "Portable.") + language.GetKindName(tree.Kind(n)), span = Span(tree.Span(n)) }).ToArray(),
            bindings = binding.References.Select(r => {
                var selected = project.Resolve(r).Single();
                return calls && selected.IsBuiltin ? (object)new { span = Span(r.NameSpan), builtin = selected.Name, kind = selected.Kind } : (namespaces || inference || calls) ? (object)new { span = Span(r.NameSpan), symbol = Span(selected.NameSpan)[0], kind = selected.Kind }
                    : new { span = Span(r.NameSpan), symbol = Span(selected.NameSpan)[0] };
            }).ToArray(),
            hir = lowered.Roots.Select(Hir).ToArray(), values, trace };
    }
}
