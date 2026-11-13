using System.Diagnostics;
using System.Diagnostics.Tracing;
using System.Runtime.CompilerServices;
using Microsoft.Diagnostics.NETCore.Client;
using Xunit;

namespace PerfhawkDotnet.Tests;

public static class Workload
{
    public static readonly object Gate = new();

    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void SpinWork(CancellationToken ct)
    {
        long x = 0;
        while (!ct.IsCancellationRequested)
            for (int i = 0; i < 100_000; i++) x += i * i;
        GC.KeepAlive(x);
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void AllocateStuff(CancellationToken ct)
    {
        var keep = new List<byte[]>();
        while (!ct.IsCancellationRequested)
        {
            keep.Add(new byte[1024]);
            if (keep.Count > 5000) keep.Clear();
        }
    }

    [MethodImpl(MethodImplOptions.NoInlining)]
    public static void FightOverLock(CancellationToken ct)
    {
        while (!ct.IsCancellationRequested)
        {
            lock (Gate) { Thread.Sleep(2); }
        }
    }

    public static (CancellationTokenSource Cts, List<Thread> Threads) Start()
    {
        var cts = new CancellationTokenSource();
        var threads = new List<Thread>
        {
            new(() => SpinWork(cts.Token)) { IsBackground = true, Name = "spin" },
            new(() => AllocateStuff(cts.Token)) { IsBackground = true, Name = "alloc" },
        };
        for (int i = 0; i < 4; i++)
            threads.Add(new Thread(() => FightOverLock(cts.Token)) { IsBackground = true, Name = $"lock{i}" });
        foreach (var t in threads) t.Start();
        return (cts, threads);
    }
}

public class ConverterTests
{
    private static string TempDir()
    {
        var d = Path.Combine(Path.GetTempPath(), "perfhawk-dotnet-tests-" + Guid.NewGuid().ToString("N"));
        Directory.CreateDirectory(d);
        return d;
    }

    [Fact]
    public async Task CollectsAllKindsFromOwnProcess()
    {
        var (cts, threads) = Workload.Start();
        var outDir = TempDir();
        try
        {
            var kinds = new HashSet<string> { "cpu", "wall", "memory", "lock", "threads" };
            var client = new DiagnosticsClient(Environment.ProcessId);
            var result = await Collector.CollectAsync(client, 2000, kinds, outDir, CancellationToken.None, null);

            Assert.Equal("complete", result.End);
            Assert.True(File.Exists(Path.Combine(outDir, "trace.nettrace")));
            var byKind = result.Artifacts.ToDictionary(a => a.Kind);

            var wall = PprofDecoder.DecodeGzip(byKind["wall"].Path);
            Assert.Equal(("wall", "nanoseconds"), wall.SampleTypes.Single());
            Assert.True(wall.Samples.Count > 0, "wall samples");
            Assert.Equal(wall.Samples.Count, byKind["wall"].Samples);

            var mem = PprofDecoder.DecodeGzip(byKind["memory"].Path);
            Assert.Equal(new[] { "alloc_space", "alloc_objects" }, mem.SampleTypes.Select(t => t.Type).ToArray());
            Assert.Equal("alloc_space", mem.DefaultSampleType);
            Assert.True(mem.Samples.Count > 0, "memory samples");
            Assert.Contains(mem.Samples, s => mem.FramesOf(s).Any(f => f.Contains("AllocateStuff")));

            var lck = PprofDecoder.DecodeGzip(byKind["lock"].Path);
            Assert.Equal(new[] { "contentions", "delay" }, lck.SampleTypes.Select(t => t.Type).ToArray());
            Assert.True(lck.Samples.Count >= 1, "lock samples");
            Assert.All(lck.Samples, s => Assert.True(s.Values[1] >= 0));

            var thr = PprofDecoder.DecodeGzip(byKind["threads"].Path);
            Assert.Equal(("threads", "count"), thr.SampleTypes.Single());
            Assert.True(thr.Samples.Count >= 2, "thread samples");
            Assert.All(thr.Samples, s => Assert.Equal("thread", s.Labels.Single().Key));

            Assert.True(byKind.ContainsKey("cpu"));
            var cpu = PprofDecoder.DecodeGzip(byKind["cpu"].Path);
            Assert.Equal(("cpu", "nanoseconds"), cpu.SampleTypes.Single());
        }
        finally
        {
            cts.Cancel();
            foreach (var t in threads) t.Join(2000);
            try { Directory.Delete(outDir, true); } catch { }
        }
    }

    [Fact]
    public async Task BucketLabelsAreWrittenOnlyWhenRequestedAndSpreadOverTime()
    {
        var (cts, threads) = Workload.Start();
        var outDir = TempDir();
        var plainDir = TempDir();
        try
        {
            var client = new DiagnosticsClient(Environment.ProcessId);
            var kinds = new HashSet<string> { "cpu", "wall", "memory" };
            var timed = await Collector.CollectAsync(client, 2000, kinds, outDir, CancellationToken.None, null, null, bucketMs: 20);
            foreach (var kind in new[] { "cpu", "wall" })
            {
                var prof = PprofDecoder.DecodeGzip(timed.Artifacts.Single(a => a.Kind == kind).Path);
                Assert.True(prof.Samples.Count > 0, kind);
                Assert.All(prof.Samples, s => Assert.Equal("perfhawk.bucket", s.Labels.Single().Key));
                var buckets = prof.Samples.Select(s => int.Parse(s.Labels.Single().Str)).Distinct().ToList();
                // 2 s at 20 ms is up to 100 buckets; sampling jitter and startup lose a few.
                Assert.True(buckets.Count >= 20, $"{kind}: only {buckets.Count} distinct buckets");
                Assert.All(buckets, b => Assert.InRange(b, 0, 400));
            }
            var mem = PprofDecoder.DecodeGzip(timed.Artifacts.Single(a => a.Kind == "memory").Path);
            Assert.DoesNotContain(mem.Samples, s => s.Labels.Any(l => l.Key == "perfhawk.bucket"));

            var plain = await Collector.CollectAsync(client, 1000, new HashSet<string> { "cpu" }, plainDir, CancellationToken.None, null);
            var cpu = PprofDecoder.DecodeGzip(plain.Artifacts.Single().Path);
            Assert.All(cpu.Samples, s => Assert.Empty(s.Labels));
        }
        finally
        {
            cts.Cancel();
            foreach (var t in threads) t.Join(2000);
            try { Directory.Delete(outDir, true); } catch { }
            try { Directory.Delete(plainDir, true); } catch { }
        }
    }

    [Fact]
    public async Task OnlyRequestedKindsAreProduced()
    {
        var (cts, threads) = Workload.Start();
        var outDir = TempDir();
        try
        {
            var client = new DiagnosticsClient(Environment.ProcessId);
            var result = await Collector.CollectAsync(client, 1000, new HashSet<string> { "wall" }, outDir, CancellationToken.None, null);
            Assert.Equal(new[] { "wall" }, result.Artifacts.Select(a => a.Kind).ToArray());
        }
        finally
        {
            cts.Cancel();
            foreach (var t in threads) t.Join(2000);
            try { Directory.Delete(outDir, true); } catch { }
        }
    }

    [Fact]
    public async Task StopTokenEndsCaptureEarlyAsCancelled()
    {
        var (cts, threads) = Workload.Start();
        var outDir = TempDir();
        try
        {
            var client = new DiagnosticsClient(Environment.ProcessId);
            using var stop = new CancellationTokenSource(TimeSpan.FromMilliseconds(1000));
            var sw = Stopwatch.StartNew();
            var result = await Collector.CollectAsync(client, 60_000, new HashSet<string> { "wall" }, outDir, stop.Token, null);
            Assert.True(sw.Elapsed < TimeSpan.FromSeconds(30), $"took {sw.Elapsed}");
            Assert.Equal("cancelled", result.End);
            Assert.Single(result.Artifacts);
        }
        finally
        {
            cts.Cancel();
            foreach (var t in threads) t.Join(2000);
            try { Directory.Delete(outDir, true); } catch { }
        }
    }
}
