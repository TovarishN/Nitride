// args: <source> <edits.json> [repeats]. Applies the edit script in order with WithChangedText + GetRoot,
// after one untimed pass over the script to warm the JIT. Reports per-edit seconds (median, p90, max).
using System.Diagnostics;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
var source = File.ReadAllText(args[0]);
var script = JsonDocument.Parse(File.ReadAllText(args[1])).RootElement.GetProperty("edits");
var edits = script.EnumerateArray().Select(e => {
    var r = e.GetProperty("range"); int s = r[0].GetInt32(), t = r[1].GetInt32();
    return new TextChange(new TextSpan(s, t - s), e.GetProperty("replacement").GetString()!);
}).ToList();
var options = new CSharpParseOptions(LanguageVersion.CSharp4);
List<double> Pass(bool timed) {
    var tree = CSharpSyntaxTree.ParseText(SourceText.From(source), options); tree.GetRoot();
    var times = new List<double>();
    foreach (var change in edits) {
        var w = Stopwatch.StartNew();
        var text = tree.GetText().WithChanges(change);
        tree = tree.WithChangedText(text); tree.GetRoot();
        w.Stop(); if (timed) times.Add(w.Elapsed.TotalSeconds);
    }
    return times;
}
Pass(false);
var all = new List<double>();
int repeats = args.Length > 2 ? int.Parse(args[2]) : 1;
for (int i = 0; i < repeats; i++) all.AddRange(Pass(true));
all.Sort();
Console.WriteLine(JsonSerializer.Serialize(new { edits = all.Count, median = all[all.Count / 2], p90 = all[all.Count * 9 / 10], max = all[^1], mean = all.Average() }));
