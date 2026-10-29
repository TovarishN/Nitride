using System.Text.Json;
using Nitride.Reference;

try
{
    if (args is ["export-project", var project, var destination])
        File.WriteAllText(destination, Exporter.ExportProject(project) + "\n");
    else if (args is ["export", var grammar, var catalog, var output])
        File.WriteAllText(output, Exporter.Export(File.ReadAllText(grammar), Catalogs.Read(catalog)) + "\n");
    else if (args is ["oracle-syntax", var profile, var sourcePath, var metadata])
        Console.WriteLine(JsonSerializer.Serialize(SyntaxOracle.Observe(profile, File.ReadAllText(sourcePath), File.ReadAllText(metadata))));
    else if (args is ["oracle-power", var powerSource, var powerCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(powerSource), Catalogs.Read(powerCatalog), true)));
    else if (args is ["oracle-namespaces", var namespaceSource, var namespaceCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(namespaceSource), Catalogs.Read(namespaceCatalog), namespaces: true)));
    else if (args is ["oracle-inference", var inferredSource, var inferredCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(inferredSource), Catalogs.Read(inferredCatalog), inference: true)));
    else if (args is ["oracle-calls", var callSource, var callCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(callSource), Catalogs.Read(callCatalog), calls: true)));
    else if (args is ["oracle-functions", var functionSource, var functionCatalog])
        Console.WriteLine(JsonSerializer.Serialize(FunctionsOracle.Observe(File.ReadAllText(functionSource), Catalogs.Read(functionCatalog))));
    else if (args is ["oracle-dates", var dateSource, var dateCatalog, var today])
        Console.WriteLine(JsonSerializer.Serialize(DatesOracle.Observe(File.ReadAllText(dateSource), Catalogs.Read(dateCatalog), today)));
    else if (args is ["oracle-rewrites", var rewriteSource, var rewriteCatalog, var rewriteToday])
        Console.WriteLine(JsonSerializer.Serialize(DatesOracle.Observe(File.ReadAllText(rewriteSource), Catalogs.Read(rewriteCatalog), rewriteToday, rewrites: true)));
    else if (args is ["bench-syntax", var benchProfile, var benchSource, var benchRuns])
        Console.WriteLine(JsonSerializer.Serialize(SyntaxOracle.Bench(benchProfile, File.ReadAllText(benchSource), int.Parse(benchRuns))));
    else if (args is ["oracle", var source, var operations])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(source), Catalogs.Read(operations))));
    else throw new ArgumentException("usage: export <grammar> <catalog> <output> | oracle <source> <catalog>");
}
catch (Exception error)
{
    Console.Error.WriteLine(error.Message);
    Environment.ExitCode = 1;
}
