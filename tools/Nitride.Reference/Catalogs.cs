using System.Diagnostics;

namespace Nitride.Reference;

// Catalogs are .ncatalog files that only the native toolchain parses. These tools
// read the catalog JSON that `nitride catalog` lowers them to; NITRIDE names the
// nitride executable, else it runs from the workspace through cargo.
static class Catalogs
{
    public static string Read(string path) => path.EndsWith(".ncatalog", StringComparison.OrdinalIgnoreCase) ? Lower([path]) : File.ReadAllText(path);

    public static string Lower(IReadOnlyList<string> paths)
    {
        var nitride = Environment.GetEnvironmentVariable("NITRIDE");
        var start = nitride is { Length: > 0 } ? new ProcessStartInfo(nitride) : new ProcessStartInfo("cargo") { WorkingDirectory = Workspace() };
        if (nitride is not { Length: > 0 })
            foreach (var arg in new[] { "run", "-q", "-p", "nitride-cli", "--bin", "nitride", "--" }) start.ArgumentList.Add(arg);
        start.ArgumentList.Add("catalog");
        foreach (var path in paths) start.ArgumentList.Add(Path.GetFullPath(path));
        start.RedirectStandardOutput = true;
        start.RedirectStandardError = true;
        using var process = Process.Start(start)!;
        var output = process.StandardOutput.ReadToEndAsync();
        var error = process.StandardError.ReadToEnd();
        process.WaitForExit();
        if (process.ExitCode != 0) throw new InvalidOperationException("nitride catalog failed: " + error.Trim());
        return output.Result.TrimEnd('\r', '\n');
    }

    static string Workspace()
    {
        foreach (var from in new[] { Environment.CurrentDirectory, AppContext.BaseDirectory })
            for (var dir = new DirectoryInfo(from); dir is not null; dir = dir.Parent)
                if (File.Exists(Path.Combine(dir.FullName, "Cargo.toml")) && Directory.Exists(Path.Combine(dir.FullName, "crates")))
                    return dir.FullName;
        throw new InvalidOperationException("set NITRIDE to the nitride executable");
    }
}
