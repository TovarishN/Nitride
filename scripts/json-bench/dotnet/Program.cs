using System.Diagnostics;
using System.Text.Json;

// Mirrors nitride-json-bench: `check` gives one verdict per file and `bench`
// times the first parse in this process (including JIT) and then warm parses.
// `comments` accepts the comments Nitride's fixed trivia skips.
var options = new JsonDocumentOptions { MaxDepth = 100_000 };
var mode = args[0];
if (mode is "check" or "check-comments")
{
    if (mode == "check-comments")
        options.CommentHandling = JsonCommentHandling.Skip;
    foreach (var path in args[1..])
    {
        var bytes = File.ReadAllBytes(path);
        string verdict;
        try { using var _ = JsonDocument.Parse(bytes, options); verdict = "accept"; }
        catch (JsonException) { verdict = "reject"; }
        Console.WriteLine($"{verdict}\t{path}");
    }
    return;
}

// `cross-document|cross-reader <warm-file> <warm> <target-file>`: warm parses of
// one file, then the first and later parses of another.
if (mode.StartsWith("cross-"))
{
    var warmup = File.ReadAllBytes(args[1]);
    var target = File.ReadAllBytes(args[3]);
    var document = mode == "cross-document";
    long Run(byte[] bytes)
    {
        if (document) { using var d = JsonDocument.Parse(bytes, options); return d.RootElement.GetArrayLength(); }
        var reader = new Utf8JsonReader(bytes, new JsonReaderOptions { MaxDepth = 100_000 });
        long tokens = 0;
        while (reader.Read()) tokens++;
        return tokens;
    }
    for (var i = 0; i < int.Parse(args[2]); i++) Run(warmup);
    var times = new long[11];
    for (var i = 0; i < times.Length; i++)
    {
        var start = Stopwatch.GetTimestamp();
        Run(target);
        times[i] = (long)Stopwatch.GetElapsedTime(start).TotalNanoseconds;
    }
    var later = times[1..];
    Array.Sort(later);
    Console.WriteLine(JsonSerializer.Serialize(new Dictionary<string, object?>
    {
        ["first_ns"] = times[0], ["repeat_median_ns"] = later[later.Length / 2],
    }));
    return;
}

var source = File.ReadAllBytes(args[1]);
var warm = int.Parse(args[2]);
Func<long> parse = mode switch
{
    // Builds JsonDocument's index of every token, the closest analogue of a syntax tree.
    "document" => () => { using var d = JsonDocument.Parse(source, options); return d.RootElement.GetArrayLength(); },
    // Validates and tokenizes without retaining anything.
    "reader" => () =>
    {
        var reader = new Utf8JsonReader(source, new JsonReaderOptions { MaxDepth = 100_000 });
        long tokens = 0;
        while (reader.Read()) tokens++;
        return tokens;
    },
    _ => throw new ArgumentException("usage: check|check-comments <file>... | document|reader <file> <warm>"),
};
var samples = new long[warm + 1];
long result = 0;
for (var i = 0; i <= warm; i++)
{
    var start = Stopwatch.GetTimestamp();
    result = parse();
    samples[i] = (long)Stopwatch.GetElapsedTime(start).TotalNanoseconds;
}
var rest = samples[1..];
Array.Sort(rest);
Console.WriteLine(JsonSerializer.Serialize(new Dictionary<string, object?>
{
    ["engine"] = $"stj-{mode}", ["source_bytes"] = source.Length, ["result"] = result,
    ["cold_ns"] = samples[0], ["warm_median_ns"] = rest.Length > 0 ? rest[rest.Length / 2] : null,
    ["warm_min_ns"] = rest.Length > 0 ? rest[0] : null,
}));
