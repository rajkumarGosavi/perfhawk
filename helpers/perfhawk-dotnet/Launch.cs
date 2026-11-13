using System.Diagnostics;
using Microsoft.Diagnostics.NETCore.Client;

namespace PerfhawkDotnet;

public static class Launcher
{
    /// <summary>
    /// Starts <paramref name="program"/> suspended behind a diagnostic port, begins the
    /// session, then resumes the runtime so startup is captured too.
    /// </summary>
    public static async Task<CollectResult> LaunchAsync(
        string program,
        IReadOnlyList<string> args,
        int durationMs,
        ISet<string> kinds,
        string outDir,
        CancellationToken stop,
        Action<string>? progress,
        int bucketMs = 0)
    {
        var address = OperatingSystem.IsWindows()
            ? "perfhawk-" + Guid.NewGuid().ToString("N")
            : Path.Combine(Path.GetTempPath(), "perfhawk-" + Guid.NewGuid().ToString("N") + ".sock");

        var psi = new ProcessStartInfo(program)
        {
            UseShellExecute = false,
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
        };
        foreach (var a in args) psi.ArgumentList.Add(a);
        psi.Environment["DOTNET_DiagnosticPorts"] = address + ",suspend";

        Process child;
        try
        {
            child = Process.Start(psi) ?? throw new InvalidOperationException("no process");
        }
        catch (Exception e)
        {
            throw new HelperException("launch_failed", $"could not start '{program}': {e.Message}");
        }

        // Keep the child's stdout off ours: stdout carries the protocol.
        child.StandardInput.Close();
        _ = Task.Run(async () =>
        {
            try
            {
                string? line;
                while ((line = await child.StandardOutput.ReadLineAsync()) != null)
                    Console.Error.WriteLine(line);
            }
            catch { /* child gone */ }
        });

        try
        {
            progress?.Invoke($"started {program} (pid {child.Id}); waiting for its diagnostic connection");
            DiagnosticsClientConnector? connector;
            using (var accept = CancellationTokenSource.CreateLinkedTokenSource(stop))
            {
                accept.CancelAfter(TimeSpan.FromSeconds(15));
                try
                {
                    // Returns null (rather than throwing) when cancelled before a connection arrives.
                    connector = await DiagnosticsClientConnector.FromDiagnosticPort(address, accept.Token);
                }
                catch (OperationCanceledException)
                {
                    connector = null;
                }
            }
            if (connector is null)
            {
                if (stop.IsCancellationRequested) return new CollectResult("cancelled", new());
                throw new HelperException("launch_failed",
                    $"'{program}' did not connect to perfhawk; is it a .NET 8+ program, and is DOTNET_EnableDiagnostics unset?");
            }

            await using (connector)
            {
                var client = connector.Instance;
                return await Collector.CollectAsync(client, durationMs, kinds, outDir, stop, progress,
                    afterStart: () => client.ResumeRuntime(), bucketMs: bucketMs);
            }
        }
        finally
        {
            try { if (!child.HasExited) child.Kill(entireProcessTree: true); } catch { }
            try { if (!OperatingSystem.IsWindows() && File.Exists(address)) File.Delete(address); } catch { }
        }
    }
}
