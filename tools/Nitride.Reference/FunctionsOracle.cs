using System.Text;
using System.Text.Json.Nodes;
using Nitrogen;
using Nitrogen.Binding;
using Nitrogen.Semantic;
using Nitrogen.Semantics;
using Nitride.Reference.Functions;

namespace Nitride.Reference;

static class FunctionsOracle
{
    public static object Observe(string source, string catalogText)
    {
        var catalog=JsonNode.Parse(catalogText)!;
        var types=catalog["types"]!.AsArray().Select(t=>(string)t!).ToDictionary(t=>t,t=>SemanticType.Named(t[..t.LastIndexOf('.')],t[(t.LastIndexOf('.')+1)..]));
        var operations=catalog["operations"]!.AsArray().Select(o=>new OperationSignature((string)o!["id"]!,types[(string)o["result"]!],o["inputs"]!.AsArray().Select(t=>types[(string)t!]).ToArray())).ToArray();
        var language=new LanguageBuilder().Add(FunctionsModule.Instance).AddSemantic(new SemanticModule("Functions",[],types.Values,operations)).Build();
        using var parsed=language.Parse(source,FunctionsModule.Program);
        int[] Span(TextSpan s)=>[Encoding.UTF8.GetByteCount(source.AsSpan(0,s.Start)),Encoding.UTF8.GetByteCount(source.AsSpan(0,s.End))];
        object Error(string category,TextSpan span,string message)=>new{ok=false,error=category,span=Span(span),message,trace=Array.Empty<string>()};
        if(!parsed.Success)return Error("syntax",parsed.Diagnostics[0].Span,"invalid source");
        var tree=parsed.Tree;var project=new Project(language);var binding=project.Set("input.functions",tree);
        var bindErrors=project.Diagnostics("input.functions");if(bindErrors.Count>0)return Error("binding",bindErrors[0].Span,bindErrors[0].Message);
        using var semantics=new ProjectSemantics(project);var file=semantics["input.functions"];
        var typeErrors=file.Diagnostics();if(typeErrors.Count>0)return Error("type",typeErrors[0].Span,typeErrors[0].Message);
        var lowered=HirLowering.LowerSelected(file,new HashSet<int>{FunctionsKinds.Show},Guid.NewGuid());
        if(lowered.Diagnostics.Count>0)return Error("lowering",lowered.Diagnostics[0].Origin.Span,lowered.Diagnostics[0].Message);
        var signatures=operations.ToDictionary(o=>o.Id);
        _=HostOperationRegistry.Bind(language.SemanticCatalog,operations.Select(o=>new HostOperationBinding(o,(Func<IReadOnlyList<ExecutionValue>,ExecutionValue>)(a=>new ExecutionValue(o.Result,0)))));
        foreach(var node in lowered.Roots.SelectMany(HirTraversal.PreOrder))
            if(node is HirOperation o && (!signatures.TryGetValue(o.Signature.Id,out var signature)||!signature.Equals(o.Signature)))return Error("host",node.Origins[0].Span,"signature mismatch");
        var trace=new List<string>();var values=new List<double>();
        double Eval(HirNode node)
        {
            double value;
            if(node is HirConstant n)value=n.Value;
            else if(node is HirOperation op)
            {
                var args=op.Arguments.Select(Eval).ToArray();trace.Add(op.Signature.Id);
                string handler=(string)catalog["operations"]!.AsArray().Single(o=>(string)o!["id"]! == op.Signature.Id)!["handler"]!;
                value=handler switch{"add"=>args[0]+args[1],"identity"=>args[0],"neg"=>-args[0],_=>throw new Exception("unknown handler")};
            }
            else throw new Exception("unexpected unexpanded HIR "+node.GetType().Name);
            if(!double.IsFinite(value))throw new ArithmeticException("nonfinite result");return value;
        }
        object Hir(HirNode node)=>node switch {
            HirConstant n=>new{kind="constant",type=node.Type.Id,span=Span(node.Origins[0].Span),origins=node.Origins.Select(o=>Span(o.Span)).ToArray(),number=(double)n.Value},
            HirOperation o=>new{kind="operation",type=node.Type.Id,span=Span(node.Origins[0].Span),origins=node.Origins.Select(o=>Span(o.Span)).ToArray(),operation=o.Signature.Id,args=o.Arguments.Select(Hir).ToArray()},
            _=>throw new Exception("unexpected HIR")};
        try{foreach(var root in lowered.Roots)values.Add(Eval(root));}
        catch(ArithmeticException e){return new{ok=false,error="execution",message=e.Message,trace};}
        var kinds=new HashSet<string>{"Definition","Parameter","Alias","Show","Num","Ref","Paren","Apply","Add","AddTime","Duration","Neg"};
        return new{ok=true,syntax=Enumerable.Range(0,tree.NodeCount).Where(n=>kinds.Contains(language.GetKindName(tree.Kind(n)))).Select(n=>new{kind="Functions."+language.GetKindName(tree.Kind(n)),span=Span(tree.Span(n))}).ToArray(),
            bindings=binding.References.Select(r=>new{kind=project.Resolve(r).Single().Kind,span=Span(r.NameSpan),symbol=Span(project.Resolve(r).Single().NameSpan)[0]}).ToArray(),hir=lowered.Roots.Select(Hir).ToArray(),values,trace};
    }
}
