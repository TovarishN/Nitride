using System.Diagnostics;
using System.Security.Cryptography;
using System.Text;
using System.Text.Json;
using Microsoft.CodeAnalysis;
using Microsoft.CodeAnalysis.CSharp;
using Microsoft.CodeAnalysis.CSharp.Syntax;

while (Console.ReadLine() is { } line)
{
    using var request = JsonDocument.Parse(line);
    var source = request.RootElement.GetProperty("source").GetString()!;
    var clock = Stopwatch.StartNew();
    var tree = CSharpSyntaxTree.ParseText(source, new CSharpParseOptions(LanguageVersion.CSharp4));
    var root = tree.GetRoot();
    clock.Stop();
    var offsets = Utf8Offsets(source);
    int[] Span(Microsoft.CodeAnalysis.Text.TextSpan span) => [offsets[span.Start], offsets[span.End]];
    var items = root.DescendantNodes().Where(node => node is MemberDeclarationSyntax or StatementSyntax)
        .Select(node => new { span = Span(node.Span), shape = Fingerprint(node), kind = node.Kind().ToString(), skipped = node.ContainsSkippedText }).ToArray();
    var tokens = root.DescendantTokens().Where(token => !token.IsMissing && token.Span.Length > 0)
        .Select(token => new { span = Span(token.Span), text = token.Text }).ToArray();
    var diagnostics = tree.GetDiagnostics().Where(d => d.Severity == DiagnosticSeverity.Error)
        .Select(d => new { span = Span(d.Location.SourceSpan), code = d.Id, message = d.GetMessage() }).ToArray();
    Console.WriteLine(JsonSerializer.Serialize(new {
        version = typeof(CSharpSyntaxTree).Assembly.GetName().Version!.ToString(),
        items, tokens, diagnostics, parse_ns = (long)clock.Elapsed.TotalNanoseconds
    }));
}

static int[] Utf8Offsets(string source)
{
    var result = new int[source.Length + 1];
    int bytes = 0;
    for (int i = 0; i < source.Length; i++)
    {
        result[i] = bytes;
        if (char.IsHighSurrogate(source[i]) && i + 1 < source.Length && char.IsLowSurrogate(source[i + 1]))
        {
            result[++i] = bytes;
            bytes += 4;
        }
        else bytes += source[i] <= 0x7f ? 1 : source[i] <= 0x7ff ? 2 : 3;
    }
    result[source.Length] = bytes;
    return result;
}

static string Fingerprint(SyntaxNode root)
{
    using var stream = new MemoryStream();
    using var writer = new BinaryWriter(stream, Encoding.UTF8, leaveOpen: true);
    var pending = new Stack<SyntaxNodeOrToken>();
    pending.Push(root);
    while (pending.TryPop(out var value))
    {
        writer.Write(value.RawKind);
        writer.Write(value.IsNode);
        if (value.IsToken)
        {
            var token = value.AsToken();
            writer.Write(token.IsMissing);
            writer.Write(token.Text);
        }
        else
        {
            var children = value.AsNode()!.ChildNodesAndTokens();
            writer.Write(children.Count);
            for (int i = children.Count - 1; i >= 0; i--) pending.Push(children[i]);
        }
    }
    writer.Flush();
    return Convert.ToHexString(SHA256.HashData(stream.GetBuffer().AsSpan(0, (int)stream.Length)));
}
