// args: cold|warm <source>. Times Language.Parse (fast pass, recovery when it fails).
// Language construction is timed separately. Retained: managed heap growth with the result alive.
using System.Diagnostics;
using System.Reflection;
using System.Text.Json;
using Nitrogen;
var source = File.ReadAllText(args[1]);
var assembly = Assembly.Load("Nitride.Reference");
var build = Stopwatch.StartNew();
var builder = new LanguageBuilder();
foreach (var t in assembly.GetTypes().Where(t => t.Namespace == "Nitride.Reference.CSharp4" && typeof(SyntaxModule).IsAssignableFrom(t) && !t.IsAbstract))
{
    var instance = t.GetProperty("Instance", BindingFlags.Public | BindingFlags.Static)?.GetValue(null)
        ?? t.GetField("Instance", BindingFlags.Public | BindingFlags.Static)?.GetValue(null);
    if (instance is SyntaxModule module) builder.Add(module);
}
var language = builder.Build();
var unit = assembly.GetType("Nitride.Reference.CSharp4.CompilationUnitsModule")!.GetMember("CompilationUnit").Single();
var start = (Rule)(unit is FieldInfo f ? f.GetValue(null)! : ((PropertyInfo)unit).GetValue(null)!);
build.Stop();
int warmups = args[0] != "warm" ? 0 : args.Length > 3 ? int.Parse(args[2]) : 2, runs = args[0] != "warm" ? 1 : args.Length > 3 ? int.Parse(args[3]) : 5;
for (int i = 0; i < warmups; i++) language.Parse(source, start).Dispose();
var times = new List<double>();
for (int i = 0; i < runs; i++) { var w = Stopwatch.StartNew(); using var r = language.Parse(source, start); w.Stop(); times.Add(w.Elapsed.TotalSeconds); }
GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
long before = GC.GetTotalMemory(true);
var held = language.Parse(source, start);
long after = GC.GetTotalMemory(true);
var diagnostics = held.Diagnostics.Length;
// Pooled rentals don't show as heap growth: sum the arrays the tree holds, at their
// rented length (as bench-syntax does), plus its UTF-16 text.
long arrays = 0;
foreach (var field in typeof(SyntaxTree).GetFields(BindingFlags.Instance | BindingFlags.NonPublic))
    if (field.Name != "_kindOf" && field.GetValue(held.Tree) is Array array)
        arrays += (long)array.Length * System.Runtime.InteropServices.Marshal.SizeOf(array.GetType().GetElementType()!);
var nodes = held.Tree.NodeCount;
var success = held.Success;
if (Environment.GetEnvironmentVariable("NDIAG") is not null) foreach (var d in held.Diagnostics) Console.Error.WriteLine($"{d.Code} {held.FormatMessage(d)} nodes={held.Tree.NodeCount}");
GC.KeepAlive(held);
held.Dispose();
times.Sort();
Console.WriteLine(JsonSerializer.Serialize(new { seconds = times[times.Count / 2], samples = times, retained_bytes = arrays + 2L * source.Length, heap_growth_bytes = after - before, nodes, diagnostics, success, language_build_seconds = build.Elapsed.TotalSeconds }));
