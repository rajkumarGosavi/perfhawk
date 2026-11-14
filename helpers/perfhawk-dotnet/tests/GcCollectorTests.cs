using System.Text.Json;
using PerfhawkDotnet.Convert;
using Xunit;

namespace PerfhawkDotnet.Tests;

public class GcCollectorTests
{
    private const double Ms = 1_000_000.0;

    private static JsonElement Build(GcCollector c, long durationMs = 5000)
    {
        using var doc = JsonDocument.Parse(c.BuildJson(1_700_000_000_000_000_000, (long)(durationMs * Ms)));
        return doc.RootElement.Clone();
    }

    [Fact]
    public void BlockingGcPauseSpansSuspendToRestart()
    {
        var c = new GcCollector();
        c.OnSuspendStart(100);
        c.OnGcStart(1, 0, "AllocSmall", "NonConcurrentGC", 101);
        c.OnGcEnd(1, 0, 110);
        c.OnRestartStop(112);
        var e = Build(c).GetProperty("events")[0];
        Assert.Equal(12 * Ms, e.GetProperty("pause_ns").GetDouble(), 1);
        // The event spans suspend to restart so the pause never exceeds the duration.
        Assert.True(e.GetProperty("duration_ns").GetDouble() >= e.GetProperty("pause_ns").GetDouble());
        Assert.Equal(100 * Ms, e.GetProperty("start_ns").GetDouble(), 1);
        Assert.Equal("blocking", e.GetProperty("kind").GetString());
        Assert.Equal(0, e.GetProperty("generation").GetInt32());
    }

    [Fact]
    public void BackgroundGcSumsItsShortPauses()
    {
        var c = new GcCollector();
        c.OnSuspendStart(998);
        c.OnGcStart(1, 2, "AllocSmall", "BackgroundGC", 1000);
        c.OnRestartStop(1003);
        c.OnSuspendStart(1200);
        c.OnRestartStop(1210);
        c.OnGcEnd(1, 2, 1500);
        var e = Build(c).GetProperty("events")[0];
        Assert.Equal(15 * Ms, e.GetProperty("pause_ns").GetDouble(), 1);
        Assert.True(e.GetProperty("duration_ns").GetDouble() > 400 * Ms, "background GC runs concurrently");
        Assert.Equal("background", e.GetProperty("kind").GetString());
    }

    [Fact]
    public void OverlappingNonGcSuspensionIsIgnoredAndPairingIsFifo()
    {
        var c = new GcCollector();
        c.OnSuspendStart(0.7, forGc: false); // sample profiler suspension
        c.OnSuspendStart(1.2);               // GC asks while the profiler still holds the runtime
        c.OnRestartStop(1.35);               // closes the profiler's suspension, not the GC's
        c.OnGcStart(1, 0, "AllocSmall", "NonConcurrentGC", 1.36);
        c.OnGcEnd(1, 0, 2.09);
        c.OnRestartStop(2.10);
        var e = Build(c).GetProperty("events")[0];
        Assert.Equal(0.9 * Ms, e.GetProperty("pause_ns").GetDouble(), 1);
    }

    [Theory]
    [InlineData("AllocSmall", "allocation", false)]
    [InlineData("AllocLarge", "large-object", false)]
    [InlineData("OutOfSpaceLOH", "large-object", false)]
    [InlineData("Induced", "induced", true)]
    [InlineData("InducedNotForced", "induced", true)]
    [InlineData("LowMemory", "low-memory", false)]
    [InlineData("LowMemoryHost", "low-memory", false)]
    [InlineData("Empty", "other", false)]
    public void ReasonNormalisation(string raw, string reason, bool induced)
    {
        Assert.Equal((reason, induced), GcCollector.NormalizeReason(raw));
    }

    [Fact]
    public void InducedFlagAndRawReasonAreKept()
    {
        var c = new GcCollector();
        c.OnGcStart(1, 2, "Induced", "NonConcurrentGC", 10);
        c.OnGcEnd(1, 2, 20);
        var e = Build(c).GetProperty("events")[0];
        Assert.True(e.GetProperty("induced").GetBoolean());
        Assert.Equal("induced", e.GetProperty("reason").GetString());
        Assert.Equal("Induced", e.GetProperty("reason_raw").GetString());
    }

    [Fact]
    public void HeapStatsAttachToTheGcThatJustEnded()
    {
        var c = new GcCollector();
        c.OnGcStart(1, 0, "AllocSmall", "NonConcurrentGC", 10);
        c.OnGcEnd(1, 0, 20);
        c.OnHeapStats(21, 100, 200, 300, 400, null, 50);
        c.OnGcStart(2, 1, "AllocSmall", "NonConcurrentGC", 30);
        c.OnGcEnd(2, 1, 40);
        c.OnHeapStats(41, 10, 20, 30, 40, 5, 7);
        var root = Build(c);
        var first = root.GetProperty("events")[0];
        var second = root.GetProperty("events")[1];
        Assert.Equal(1000, first.GetProperty("heap_after_bytes").GetInt64());
        Assert.False(first.TryGetProperty("heap_before_bytes", out _), "first GC has no previous heap");
        Assert.Equal(300, first.GetProperty("gen_after").GetProperty("gen2").GetInt64());
        Assert.Equal(50, first.GetProperty("promoted_bytes").GetInt64());
        Assert.Equal(1000, second.GetProperty("heap_before_bytes").GetInt64());
        Assert.Equal(105, second.GetProperty("heap_after_bytes").GetInt64());
        Assert.Equal(5, second.GetProperty("gen_after").GetProperty("poh").GetInt64());
        var heap = root.GetProperty("heap");
        Assert.Equal(2, heap.GetArrayLength());
        Assert.Equal(1000, heap[0].GetProperty("total_bytes").GetInt64());
        Assert.Equal(21 * Ms, heap[0].GetProperty("t_ns").GetDouble(), 1);
    }

    [Fact]
    public void AllocTicksBucketPerSecond()
    {
        var c = new GcCollector();
        c.OnAllocTick(100, 1000);
        c.OnAllocTick(900, 500);
        c.OnAllocTick(2500, 700);
        var alloc = Build(c, 4000).GetProperty("alloc");
        Assert.Equal(1_000_000_000L, alloc.GetProperty("bucket_ns").GetInt64());
        var bytes = alloc.GetProperty("bytes").EnumerateArray().Select(x => x.GetInt64()).ToArray();
        Assert.Equal(new long[] { 1500, 0, 700, 0 }, bytes); // zero-filled to the capture length
    }

    [Fact]
    public void EmptyCollectorBuildsValidJson()
    {
        var root = Build(new GcCollector());
        Assert.Equal(1, root.GetProperty("schema").GetInt32());
        Assert.Equal("dotnet", root.GetProperty("runtime").GetString());
        Assert.Equal("native", root.GetProperty("fidelity").GetString());
        Assert.Equal(0, root.GetProperty("events").GetArrayLength());
        Assert.Contains("approximate", root.GetProperty("notes")[0].GetString());
    }

    [Fact]
    public void UnfinishedGcIsDropped()
    {
        var c = new GcCollector();
        c.OnGcStart(1, 0, "AllocSmall", "NonConcurrentGC", 10); // trace ended before GCEnd
        Assert.Equal(0, c.EventCount);
        Assert.Equal(0, Build(c).GetProperty("events").GetArrayLength());
    }
}
