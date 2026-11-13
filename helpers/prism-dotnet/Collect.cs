using System.Diagnostics.Tracing;
using Microsoft.Diagnostics.NETCore.Client;
using PrismDotnet.Convert;

namespace PrismDotnet;

public sealed record CollectResult(string End, List<ArtifactInfo> Artifacts);

public static class Collector
{
    // Runtime keywords: GC (allocation ticks), Loader, Jit, NGen, Contention, Stack.
    private const long RuntimeKeywords = 0x1 | 0x8 | 0x10 | 0x20 | 0x4000 | 0x40000000;

    public static IReadOnlyList<EventPipeProvider> Providers() => new[]
    {
        new EventPipeProvider("Microsoft-DotNETCore-SampleProfiler", EventLevel.Informational, 0),
        // Verbose is required for GCAllocationTick.
        new EventPipeProvider("Microsoft-Windows-DotNETRuntime", EventLevel.Verbose, RuntimeKeywords),
    };

    /// <summary>
    /// Records one EventPipe session for <paramref name="durationMs"/> (or until
    /// <paramref name="stop"/> fires or the target exits), then converts it.
    /// <paramref name="afterStart"/> runs once the session is live (used to resume a suspended launch).
    /// </summary>
    public static async Task<CollectResult> CollectAsync(
        DiagnosticsClient client,
        int durationMs,
        ISet<string> kinds,
        string outDir,
        CancellationToken stop,
        Action<string>? progress,
        Action? afterStart = null)
    {
        Directory.CreateDirectory(outDir);
        var tracePath = Path.Combine(outDir, "trace.nettrace");

        EventPipeSession session;
        try
        {
            session = client.StartEventPipeSession(Providers(), requestRundown: true, circularBufferMB: 256);
        }
        catch (Exception e) when (e is ServerNotAvailableException or UnsupportedCommandException or TimeoutException or IOException or UnauthorizedAccessException)
        {
            throw new HelperException("diagnostics_disabled", $"could not start an EventPipe session: {e.Message}");
        }

        string end;
        using (session)
        {
            var copy = Task.Run(async () =>
            {
                try
                {
                    await using var fs = File.Create(tracePath);
                    await session.EventStream.CopyToAsync(fs);
                }
                catch (Exception) { /* the stream ends abruptly when the target exits */ }
            });
            progress?.Invoke($"recording for {durationMs} ms");
            afterStart?.Invoke();

            var stopped = new TaskCompletionSource();
            using var reg = stop.Register(() => stopped.TrySetResult());
            var timer = Task.Delay(durationMs);
            var first = await Task.WhenAny(copy, timer, stopped.Task);
            if (first == copy)
            {
                end = "target_exited";
            }
            else
            {
                end = first == stopped.Task ? "cancelled" : "complete";
                try { session.Stop(); } catch { /* already gone */ }
                await Task.WhenAny(copy, Task.Delay(TimeSpan.FromSeconds(30)));
            }
        }

        progress?.Invoke("converting trace");
        List<ArtifactInfo> artifacts;
        try
        {
            artifacts = TraceConverter.Convert(tracePath, outDir, kinds);
        }
        catch (Exception e) when (e is not HelperException)
        {
            throw new HelperException("internal", $"could not convert the trace: {e.Message}");
        }
        return new CollectResult(end, artifacts);
    }
}
