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
            "csharp4" => CSharp4Modules(),
            _ => throw new ArgumentException("unknown syntax profile") };
        var builder = new LanguageBuilder();
        foreach (var module in modules) builder.Add(module);
        var language = builder.Build();
        Rule start = profile switch { "xml" => Xml.XmlModule.Document, "csharp4" => CSharp4Start(), _ => Uses.UsesModule.Doc };
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

    /// Warm parse timing and allocation of Nitrogen's generated parser (JIT warmed by one parse).
    public static object Bench(string profile, string source, int runs)
    {
        if (profile != "csharp4") throw new ArgumentException("unknown benchmark profile");
        var builder = new LanguageBuilder();
        foreach (var module in CSharp4Modules()) builder.Add(module);
        var language = builder.Build();
        var start = CSharp4Start();
        using (var warm = language.Parse(source, start)) if (!warm.Success) throw new Exception("benchmark input rejected");
        var times = new List<double>();
        long allocated = 0;
        for (int i = 0; i < runs; i++)
        {
            GC.Collect();
            long before = GC.GetAllocatedBytesForCurrentThread();
            var watch = System.Diagnostics.Stopwatch.StartNew();
            using var parsed = language.Parse(source, start);
            watch.Stop();
            allocated = GC.GetAllocatedBytesForCurrentThread() - before;
            times.Add(watch.Elapsed.TotalSeconds);
        }
        // Retained: the arrays the tree holds (pooled rentals at their rented length), measured
        // directly since warm pooled parses allocate almost nothing; and the managed heap growth
        // while the tree is alive, after full collections, as a cross-check.
        GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
        long heapBefore = GC.GetTotalMemory(true);
        var held = language.Parse(source, start);
        long heapAfter = GC.GetTotalMemory(true);
        var tree = held.Tree;
        long arrays = 0;
        foreach (var field in typeof(SyntaxTree).GetFields(System.Reflection.BindingFlags.Instance | System.Reflection.BindingFlags.NonPublic))
            if (field.Name != "_kindOf" && field.GetValue(tree) is Array array)
                arrays += (long)array.Length * System.Runtime.InteropServices.Marshal.SizeOf(array.GetType().GetElementType()!);
        var result = new
        {
            bytes = Encoding.UTF8.GetByteCount(source), seconds_min = times.Min(), allocated_bytes = allocated,
            nodes = tree.NodeCount, tree_array_bytes = arrays, text_bytes = 2L * source.Length,
            heap_growth_bytes = heapAfter - heapBefore,
        };
        GC.KeepAlive(held);
        held.Dispose();
        return result;
    }

    // The 30 generated CSharp4 modules, found by namespace rather than listed by hand.
    static SyntaxModule[] CSharp4Modules() => typeof(SyntaxOracle).Assembly.GetTypes()
        .Where(t => t.Namespace == "Nitride.Reference.CSharp4" && typeof(SyntaxModule).IsAssignableFrom(t) && !t.IsAbstract)
        .Select(t => t.GetProperty("Instance", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Static)?.GetValue(null)
            ?? t.GetField("Instance", System.Reflection.BindingFlags.Public | System.Reflection.BindingFlags.Static)?.GetValue(null))
        .OfType<SyntaxModule>().ToArray();

    static Rule CSharp4Start()
    {
        var type = typeof(SyntaxOracle).Assembly.GetType("Nitride.Reference.CSharp4.CompilationUnitsModule")!;
        var member = type.GetMember("CompilationUnit").Single();
        return (Rule)(member is System.Reflection.FieldInfo f ? f.GetValue(null)! : ((System.Reflection.PropertyInfo)member).GetValue(null)!);
    }
}
