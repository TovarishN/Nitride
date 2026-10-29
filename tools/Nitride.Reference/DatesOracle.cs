using System.Globalization;
using System.Text;
using System.Text.Json.Nodes;
using Nitrogen;
using Nitrogen.Binding;
using Nitrogen.Semantic;
using Nitrogen.Semantics;
using Nitride.Reference.Dates;

namespace Nitride.Reference;

// Built-in values come from an explicit fixture host, as DateCalc's evaluation
// profile supplies them; dates are .NET DateOnly day numbers and durations days.
static class DatesOracle
{
    public static object Observe(string source, string catalogText, string today, bool rewrites = false)
    {
        var catalog=JsonNode.Parse(catalogText)!;
        var types=catalog["types"]!.AsArray().Select(t=>(string)t!).ToDictionary(t=>t,t=>SemanticType.Named(t[..t.LastIndexOf('.')],t[(t.LastIndexOf('.')+1)..]));
        var operations=catalog["operations"]!.AsArray().Select(o=>new OperationSignature((string)o!["id"]!,types[(string)o["result"]!],o["inputs"]!.AsArray().Select(t=>types[(string)t!]).ToArray())).ToArray();
        var builder=new LanguageBuilder().Add(DatesModule.Instance);if(rewrites)builder.Add(RewritesModule.Instance);var language=builder.AddSemantic(new SemanticModule("Dates",[],types.Values,operations)).Build();
        using var parsed=language.Parse(source,DatesModule.Program);
        int[] Span(TextSpan s)=>[Encoding.UTF8.GetByteCount(source.AsSpan(0,s.Start)),Encoding.UTF8.GetByteCount(source.AsSpan(0,s.End))];
        object Error(string category,TextSpan span,string message,string? code=null)=>code is null
            ? new{ok=false,error=category,span=Span(span),message,trace=Array.Empty<string>()}
            : new{ok=false,error=category,span=Span(span),code,message,trace=Array.Empty<string>()};
        if(!parsed.Success)return Error("syntax",parsed.Diagnostics[0].Span,"invalid source");
        var tree=parsed.Tree;var project=new Project(language);var binding=project.Set("input.dates",tree);
        var bindErrors=project.Diagnostics("input.dates");if(bindErrors.Count>0)return Error("binding",bindErrors[0].Span,bindErrors[0].Message);
        using var semantics=new ProjectSemantics(project);var file=semantics["input.dates"];
        var typeErrors=file.Diagnostics();
        if(Environment.GetEnvironmentVariable("NITRIDE_ALL_DIAGNOSTICS")=="1")
            foreach(var d in typeErrors)Console.Error.WriteLine($"{d.Code} {Span(d.Span)[0]}-{Span(d.Span)[1]} {d.Message}");
        // Diagnostics whose code is a catalog check code are reported as checks.
        var checkCodes=catalog["checks"]!.AsObject().SelectMany(r=>r.Value!.AsArray().Select(c=>(string)c!["code"]!)).ToHashSet();
        if(typeErrors.Count>0)return checkCodes.Contains(typeErrors[0].Code)
            ? Error("check",typeErrors[0].Span,typeErrors[0].Message,typeErrors[0].Code)
            : Error("type",typeErrors[0].Span,typeErrors[0].Message);
        var kinds=new HashSet<int>{DatesKinds.Let,DatesKinds.Show};
        var lowered=HirLowering.LowerSelected(file,kinds,Guid.NewGuid());
        if(lowered.Diagnostics.Count>0)return Error("type",lowered.Diagnostics[0].Origin.Span,lowered.Diagnostics[0].Message);
        var statements=Enumerable.Range(0,tree.NodeCount).Where(n=>kinds.Contains(tree.Kind(n))).ToArray();
        if(statements.Length!=lowered.Roots.Count)throw new Exception("reference lowering did not produce one root per statement");
        var declarations=binding.Declarations.ToDictionary(s=>s.Node);
        var signatures=operations.ToDictionary(o=>o.Id);
        _=HostOperationRegistry.Bind(language.SemanticCatalog,operations.Select(o=>new HostOperationBinding(o,(Func<IReadOnlyList<ExecutionValue>,ExecutionValue>)(a=>new ExecutionValue(o.Result,0)))));
        foreach(var node in lowered.Roots.SelectMany(HirTraversal.PreOrder))
            if(node is HirOperation o && (!signatures.TryGetValue(o.Signature.Id,out var signature)||!signature.Equals(o.Signature)))return Error("host",node.Origins[0].Span,"signature mismatch");
        // Built-in values bind by their catalog handler, as the native fixture host does.
        string ValueHandler(string name)=>(string)catalog["values"]!.AsObject().Select(k=>k.Value![name]).Single(v=>v is not null)!["handler"]!;
        double Builtin(string name)=>ValueHandler(name) switch{"pi"=>Math.PI,"e"=>Math.E,"tau"=>Math.Tau,"today"=>DateOnly.ParseExact(today,"yyyy-MM-dd",CultureInfo.InvariantCulture).DayNumber,var h=>throw new Exception("unknown value handler "+h)};
        var symbols=new Dictionary<Symbol,double>();var trace=new List<string>();var values=new List<double>();
        double Eval(HirNode node)
        {
            double value;
            if(node is HirConstant n)value=n.Value;
            else if(node is HirSymbolRef r)value=r.Symbol.Binding.IsBuiltin?Builtin(r.Symbol.Binding.Name):symbols[r.Symbol.Binding];
            else if(node is HirOperation op)
            {
                string handler=(string)catalog["operations"]!.AsArray().Single(o=>(string)o!["id"]! == op.Signature.Id)!["handler"]!;
                if(handler=="iso-date")
                {
                    trace.Add(op.Signature.Id);
                    value=DateOnly.ParseExact(((HirText)op.Arguments[0]).Value,"yyyy-MM-dd",CultureInfo.InvariantCulture).DayNumber;
                }
                else
                {
                    var args=op.Arguments.Select(Eval).ToArray();trace.Add(op.Signature.Id);
                    value=handler switch{"identity"=>args[0],"add"=>args[0]+args[1],"sub"=>args[0]-args[1],"mul"=>args[0]*args[1],_=>throw new Exception("unknown handler")};
                }
            }
            else throw new Exception("unsupported HIR "+node.GetType().Name);
            if(!double.IsFinite(value))throw new ArithmeticException("nonfinite result");return value;
        }
        // Rewrite profiles report every origin; template origins keep their own path and offsets.
        object Origin(SourceOrigin o)=>o.Path=="input.dates"?new{path="input",span=Span(o.Span)}:new{path=o.Path,span=new[]{o.Span.Start,o.Span.End}};
        object Hir(HirNode node)=>rewrites?RewriteHir(node):node switch {
            HirConstant c=>new{kind="constant",type=node.Type.Id,span=Span(node.Origins[0].Span),number=(double)c.Value},
            HirText t=>new{kind="text",type=node.Type.Id,span=Span(node.Origins[0].Span),text=t.Value},
            HirSymbolRef r when r.Symbol.Binding.IsBuiltin=>new{kind="reference",type=node.Type.Id,span=Span(node.Origins[0].Span),builtin=r.Symbol.Binding.Name},
            HirSymbolRef r=>(object)new{kind="reference",type=node.Type.Id,span=Span(node.Origins[0].Span),symbol=Span(r.Symbol.Binding.NameSpan)[0]},
            HirOperation o=>new{kind="operation",type=node.Type.Id,span=Span(node.Origins[0].Span),operation=o.Signature.Id,args=o.Arguments.Select(Hir).ToArray()},
            _=>throw new Exception("unsupported HIR "+node.GetType().Name)};
        object RewriteHir(HirNode node)=>node switch {
            HirConstant c=>new{kind="constant",type=node.Type.Id,origins=node.Origins.Select(Origin).ToArray(),number=(double)c.Value},
            HirText t=>new{kind="text",type=node.Type.Id,origins=node.Origins.Select(Origin).ToArray(),text=t.Value},
            HirSymbolRef r when r.Symbol.Binding.IsBuiltin=>new{kind="reference",type=node.Type.Id,origins=node.Origins.Select(Origin).ToArray(),builtin=r.Symbol.Binding.Name},
            HirSymbolRef r=>(object)new{kind="reference",type=node.Type.Id,origins=node.Origins.Select(Origin).ToArray(),symbol=Span(r.Symbol.Binding.NameSpan)[0]},
            HirOperation o=>new{kind="operation",type=node.Type.Id,origins=node.Origins.Select(Origin).ToArray(),operation=o.Signature.Id,args=o.Arguments.Select(RewriteHir).ToArray()},
            _=>throw new Exception("unsupported HIR "+node.GetType().Name)};
        try
        {
            for(int i=0;i<statements.Length;i++)
            {
                double result=Eval(lowered.Roots[i]);
                if(declarations.TryGetValue(statements[i],out var symbol))symbols[symbol]=result;else values.Add(result);
            }
        }
        catch(ArithmeticException e){return new{ok=false,error="execution",message=e.Message,trace};}
        var syntaxKinds=new HashSet<string>{"Let","Show","Day","Num","Ref","Paren","Days","InDays","Add","Sub","Mul","Fortnights","Swapped","Soon","Twice","Pad","Wrap"};
        string Module(int kind)=>rewrites&&kind>>16==RewritesModule.Instance.Id?"Rewrites.":"Dates.";
        return new{ok=true,
            syntax=Enumerable.Range(0,tree.NodeCount).Where(n=>syntaxKinds.Contains(language.GetKindName(tree.Kind(n)))).Select(n=>new{kind=Module(tree.Kind(n))+language.GetKindName(tree.Kind(n)),span=Span(tree.Span(n))}).ToArray(),
            bindings=binding.References.Select(r=>{var s=project.Resolve(r).Single();return s.IsBuiltin?(object)new{span=Span(r.NameSpan),builtin=s.Name,kind=s.Kind}:new{span=Span(r.NameSpan),symbol=Span(s.NameSpan)[0],kind=s.Kind};}).ToArray(),
            hir=lowered.Roots.Select(Hir).ToArray(),values,trace};
    }
}
