using System.Text.Json;
using Xunit;

namespace PrismDotnet.Tests;

public class CollectSmokeTests
{
    private static async Task<(int Exit, List<JsonElement> Lines)> Run(params string[] args)
    {
        var sw = new StringWriter();
        var exit = await Program.RunAsync(args, sw, new StringReader(""), CancellationToken.None);
        var lines = sw.ToString()
            .Split('\n', StringSplitOptions.RemoveEmptyEntries)
            .Select(l => JsonDocument.Parse(l).RootElement.Clone())
            .ToList();
        return (exit, lines);
    }

    [Fact]
    public async Task UnknownPidGivesNotFoundError()
    {
        var (exit, lines) = await Run("collect", "--pid", "2147483000", "--duration-ms", "1000", "--kinds", "cpu", "--out", Path.GetTempPath());
        Assert.Equal(1, exit);
        var err = lines.Single(l => l.GetProperty("type").GetString() == "error");
        Assert.Equal("not_found", err.GetProperty("code").GetString());
        Assert.DoesNotContain(lines, l => l.GetProperty("type").GetString() == "done");
    }

    [Fact]
    public async Task NonDotnetProcessGivesNotDotnetError()
    {
        // The test host's parent (e.g. dotnet CLI) may be .NET, so spawn a plain process.
        var psi = OperatingSystem.IsWindows()
            ? new System.Diagnostics.ProcessStartInfo("ping", "-n 30 127.0.0.1")
            : new System.Diagnostics.ProcessStartInfo("sleep", "30");
        psi.RedirectStandardOutput = true;
        psi.UseShellExecute = false;
        using var p = System.Diagnostics.Process.Start(psi)!;
        try
        {
            var (exit, lines) = await Run("collect", "--pid", p.Id.ToString(), "--duration-ms", "1000", "--kinds", "cpu", "--out", Path.GetTempPath());
            Assert.Equal(1, exit);
            var err = lines.Single(l => l.GetProperty("type").GetString() == "error");
            Assert.Equal("not_dotnet", err.GetProperty("code").GetString());
        }
        finally { p.Kill(true); }
    }

    [Fact]
    public async Task ListIncludesOwnProcess()
    {
        var (exit, lines) = await Run("list");
        Assert.Equal(0, exit);
        Assert.Contains(lines, l =>
            l.GetProperty("type").GetString() == "process" &&
            l.GetProperty("pid").GetInt32() == Environment.ProcessId);
    }
}
