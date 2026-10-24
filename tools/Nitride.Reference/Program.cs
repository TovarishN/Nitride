using System.Text.Json;
using Nitride.Reference;

try
{
    if (args is ["export-project", var project, var destination])
        File.WriteAllText(destination, Exporter.ExportProject(project) + "\n");
    else if (args is ["export", var grammar, var catalog, var output])
        File.WriteAllText(output, Exporter.Export(File.ReadAllText(grammar), File.ReadAllText(catalog)) + "\n");
    else if (args is ["oracle-syntax", var profile, var sourcePath, var metadata])
        Console.WriteLine(JsonSerializer.Serialize(SyntaxOracle.Observe(profile, File.ReadAllText(sourcePath), File.ReadAllText(metadata))));
    else if (args is ["oracle-power", var powerSource, var powerCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(powerSource), File.ReadAllText(powerCatalog), true)));
    else if (args is ["oracle-namespaces", var namespaceSource, var namespaceCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(namespaceSource), File.ReadAllText(namespaceCatalog), namespaces: true)));
    else if (args is ["oracle-inference", var inferredSource, var inferredCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(inferredSource), File.ReadAllText(inferredCatalog), inference: true)));
    else if (args is ["oracle-calls", var callSource, var callCatalog])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(callSource), File.ReadAllText(callCatalog), calls: true)));
    else if (args is ["oracle-functions", var functionSource, var functionCatalog])
        Console.WriteLine(JsonSerializer.Serialize(FunctionsOracle.Observe(File.ReadAllText(functionSource), File.ReadAllText(functionCatalog))));
    else if (args is ["oracle", var source, var operations])
        Console.WriteLine(JsonSerializer.Serialize(Oracle.Observe(File.ReadAllText(source), File.ReadAllText(operations))));
    else throw new ArgumentException("usage: export <grammar> <catalog> <output> | oracle <source> <catalog>");
}
catch (Exception error)
{
    Console.Error.WriteLine(error.Message);
    Environment.ExitCode = 1;
}
