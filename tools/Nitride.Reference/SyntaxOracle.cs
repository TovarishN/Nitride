using System.Text;
using System.Text.Json.Nodes;
using Nitrogen;

namespace Nitride.Reference;

static class SyntaxOracle
{
    public static object Observe(string profile, string source, string metadata)
    {
        var data = JsonNode.Parse(metadata)!;
        SyntaxModule[] modules = profile switch {
            "xml" => [Xml.XmlModule.Instance],
            "composition" => [Base.BaseModule.Instance, Uses.UsesModule.Instance],
            _ => throw new ArgumentException("unknown syntax profile") };
        var builder = new LanguageBuilder();
        foreach (var module in modules) builder.Add(module);
        var language = builder.Build();
        var start = profile == "xml" ? Xml.XmlModule.Document : Uses.UsesModule.Doc;
        using var parsed = language.Parse(source, start);
        int[] Span(TextSpan span) => [Encoding.UTF8.GetByteCount(source.AsSpan(0, span.Start)), Encoding.UTF8.GetByteCount(source.AsSpan(0, span.End))];
        if (!parsed.Success) return new { ok = false, error = "syntax", span = Span(parsed.Diagnostics[0].Span), semantic_validation = false };
        bool Alias(JsonNode e) => (string)e["kind"]! switch { "ref" => true, "label" => Alias(e["inner"]!),
            "choice" => e["items"]!.AsArray().Count > 0 && e["items"]!.AsArray().All(i => Alias(i!)), _ => false };
        var keep = data["rules"]!.AsArray().Where(r => !Alias(r!["expr"]!)).Select(r => (string)r!["name"]!).ToHashSet();
        var byId = modules.ToDictionary(m => m.Id, m => m.Name);
        var tree = parsed.Tree;
        var nodes = new List<object>();
        var stack = new Stack<(int Node, int? Parent)>();
        stack.Push((tree.Root, null));
        while (stack.TryPop(out var next))
        {
            int? parent = next.Parent;
            int kind = tree.Kind(next.Node);
            if (byId.TryGetValue(kind >> 16, out var module))
            {
                string name = module + "." + language.GetKindName(kind);
                if (keep.Contains(name)) { parent = nodes.Count; nodes.Add(new { rule = name, span = Span(tree.Span(next.Node)), parent = next.Parent }); }
            }
            for (int i = tree.ChildCount(next.Node) - 1; i >= 0; i--)
                if (tree.Child(next.Node, i) is var child && child >= 0) stack.Push((child, parent));
        }
        return new { ok = true, profile = "syntax-only", semantic_validation = false, nodes };
    }
}
