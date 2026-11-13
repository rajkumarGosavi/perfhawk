using Microsoft.Diagnostics.Tracing;
using Microsoft.Diagnostics.Tracing.EventPipe;
using Microsoft.Diagnostics.Tracing.Etlx;
using Microsoft.Diagnostics.Tracing.Parsers;
using Microsoft.Diagnostics.Tracing.Parsers.Clr;
using PerfhawkDotnet.Pprof;

namespace PerfhawkDotnet.Convert;

public sealed record ArtifactInfo(string Kind, string Path, int Samples);

/// <summary>Converts a <c>.nettrace</c> file into one pprof file per requested kind.</summary>
public static class TraceConverter
{
    public static readonly string[] AllKinds = { "cpu", "wall", "memory", "lock", "threads" };

    private const long SampleIntervalNs = 1_000_000; // SampleProfiler default: 1 ms

    /// <param name="bucketMs">When positive, cpu and wall samples get a <c>perfhawk.bucket</c> label with their time bucket (milliseconds since the trace started divided by this width).</param>
    public static List<ArtifactInfo> Convert(string nettracePath, string outDir, ISet<string> kinds, int bucketMs = 0)
    {
        Directory.CreateDirectory(outDir);
        var etlx = TraceLog.CreateFromEventPipeDataFile(nettracePath);
        try
        {
            using var log = new TraceLog(etlx);
            return ConvertLog(log, outDir, kinds, bucketMs);
        }
        finally
        {
            try { File.Delete(etlx); } catch { /* best effort */ }
        }
    }

    private static List<Frame> FramesOf(TraceEvent ev)
    {
        var frames = new List<Frame>();
        for (var cs = ev.CallStack(); cs != null; cs = cs.Caller)
        {
            var ca = cs.CodeAddress;
            string method = ca.FullMethodName;
            string module = ca.ModuleName;
            if (string.IsNullOrEmpty(method))
                frames.Add(new Frame("[unknown]", module ?? ""));
            else
                frames.Add(new Frame(string.IsNullOrEmpty(module) ? method : $"{module}!{method}", module ?? ""));
        }
        frames.Reverse(); // root first
        return frames;
    }

    private static List<ArtifactInfo> ConvertLog(TraceLog log, string outDir, ISet<string> kinds, int bucketMs)
    {
        var wall = new PprofBuilder(("wall", "nanoseconds"));
        var cpu = new PprofBuilder(("cpu", "nanoseconds"));
        var mem = new PprofBuilder(("alloc_space", "bytes"), ("alloc_objects", "count"));
        var lck = new PprofBuilder(("contentions", "count"), ("delay", "nanoseconds"));
        var thr = new PprofBuilder(("threads", "count"));
        mem.SetDefaultSampleType("alloc_space");
        lck.SetDefaultSampleType("delay");
        cpu.SetPeriod("cpu", "nanoseconds", SampleIntervalNs);
        wall.SetPeriod("wall", "nanoseconds", SampleIntervalNs);

        var lastSamplePerThread = new Dictionary<int, List<Frame>>();
        var contentionStart = new Dictionary<int, (double TimestampMs, List<Frame> Frames)>();

        var source = log.Events.GetSource();
        var sampler = new SampleProfilerTraceEventParser(source);
        var clr = new ClrTraceEventParser(source);

        sampler.ThreadSample += ev =>
        {
            var frames = FramesOf(ev);
            string? lk = null, lv = null;
            if (bucketMs > 0)
            {
                lk = "perfhawk.bucket";
                lv = ((long)Math.Max(0, ev.TimeStampRelativeMSec / bucketMs)).ToString();
            }
            if (kinds.Contains("wall")) wall.AddSample(frames, new[] { SampleIntervalNs }, lk, lv);
            if (kinds.Contains("cpu") && ev.Type == ClrThreadSampleType.Managed)
                cpu.AddSample(frames, new[] { SampleIntervalNs }, lk, lv);
            if (kinds.Contains("threads")) lastSamplePerThread[ev.ThreadID] = frames;
        };

        if (kinds.Contains("memory"))
            clr.GCAllocationTick += ev =>
                mem.AddSample(FramesOf(ev), new[] { ev.AllocationAmount64, 1L });

        if (kinds.Contains("lock"))
        {
            clr.ContentionStart += ev => contentionStart[ev.ThreadID] = (ev.TimeStampRelativeMSec, FramesOf(ev));
            clr.ContentionStop += ev =>
            {
                if (!contentionStart.Remove(ev.ThreadID, out var start)) return;
                // .NET 8+ reports the duration; otherwise pair Start/Stop timestamps.
                long delayNs = ev.DurationNs > 0
                    ? (long)ev.DurationNs
                    : (long)((ev.TimeStampRelativeMSec - start.TimestampMs) * 1_000_000.0);
                lck.AddSample(start.Frames, new[] { 1L, Math.Max(0, delayNs) });
            };
        }

        source.Process();

        if (kinds.Contains("threads"))
            foreach (var (tid, frames) in lastSamplePerThread.OrderBy(kv => kv.Key))
                thr.AddSample(frames, new[] { 1L }, "thread", tid.ToString());

        long startNs = new DateTimeOffset(log.SessionStartTime.ToUniversalTime()).ToUnixTimeMilliseconds() * 1_000_000;
        long durNs = (long)(log.SessionDuration.TotalMilliseconds * 1_000_000);
        var result = new List<ArtifactInfo>();
        void Write(string kind, PprofBuilder b)
        {
            if (!kinds.Contains(kind)) return;
            b.SetDuration(startNs, durNs);
            var path = Path.Combine(outDir, kind + ".pb.gz");
            File.WriteAllBytes(path, b.ToGzip());
            result.Add(new ArtifactInfo(kind, path, b.SampleCount));
        }
        Write("cpu", cpu);
        Write("wall", wall);
        Write("memory", mem);
        Write("lock", lck);
        Write("threads", thr);
        return result;
    }
}
