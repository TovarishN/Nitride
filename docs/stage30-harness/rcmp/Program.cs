// args: cold|warm <source>. Times ParseText + GetRoot. Retained: managed heap growth with the tree alive.
using System.Diagnostics;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.Text;
var source = File.ReadAllText(args[1]);
var options = new CSharpParseOptions(LanguageVersion.CSharp4);
int warmups = args[0] != "warm" ? 0 : args.Length > 3 ? int.Parse(args[2]) : 2, runs = args[0] != "warm" ? 1 : args.Length > 3 ? int.Parse(args[3]) : 5;
SyntaxTree Parse() { var t = CSharpSyntaxTree.ParseText(SourceText.From(source), options); t.GetRoot(); return t; }
for (int i = 0; i < warmups; i++) Parse();
var times = new List<double>();
for (int i = 0; i < runs; i++) { var w = Stopwatch.StartNew(); Parse(); w.Stop(); times.Add(w.Elapsed.TotalSeconds); }
GC.Collect(); GC.WaitForPendingFinalizers(); GC.Collect();
long before = GC.GetTotalMemory(true);
var held = Parse();
long after = GC.GetTotalMemory(true);
var errors = held.GetDiagnostics().Count(d => d.Severity == DiagnosticSeverity.Error);
GC.KeepAlive(held);
times.Sort();
Console.WriteLine(JsonSerializer.Serialize(new { seconds = times[times.Count / 2], samples = times, retained_bytes = after - before, diagnostics = errors }));
