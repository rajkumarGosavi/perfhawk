using System.Text.Json;
using System.Text.Json.Serialization;

namespace PerfhawkDotnet.Convert;

/// <summary>
/// Turns .NET GC runtime events into Perfhawk's runtime-neutral GC data (<c>gc.json</c>).
/// It takes plain values, not TraceEvent objects, so it can be tested without a trace.
/// All times are milliseconds since the trace started.
/// </summary>
public sealed class GcCollector
{
    private sealed class Gc
    {
        public int Count;
        public int Generation;
        public string ReasonRaw = "";
        public string Type = "";
        public double StartMs;
        public double? EndMs;
        public double PauseMs;
        public double FirstPauseStartMs = double.MaxValue;
        public double LastPauseEndMs = double.MinValue;
        public ulong? HeapAfter;
        public GenSizesDto? GenAfter;
        public ulong? Promoted;
    }

    private readonly List<Gc> _gcs = new();
    private readonly Dictionary<int, Gc> _open = new();
    private readonly List<(double Start, double End)> _pauses = new();
    private readonly List<HeapPointDto> _heap = new();
    private readonly List<long> _alloc = new();
    // Runtime suspensions serialise, so each RestartEEStop closes the oldest open suspension.
    // Non-GC suspensions (for example the sample profiler's) overlap GC ones and are ignored.
    private readonly Queue<(double Ms, bool ForGc)> _openSuspends = new();
    private Gc? _lastEnded;

    private const double BucketMs = 1000;

    /// <summary>Collections that have both a start and an end.</summary>
    public int EventCount => _gcs.Count(g => g.EndMs.HasValue);

    public void OnSuspendStart(double ms, bool forGc = true) => _openSuspends.Enqueue((ms, forGc));

    public void OnRestartStop(double ms)
    {
        if (!_openSuspends.TryDequeue(out var s)) return;
        if (s.ForGc) _pauses.Add((s.Ms, ms));
    }

    public void OnGcStart(int count, int generation, string reasonRaw, string type, double ms)
    {
        var g = new Gc { Count = count, Generation = generation, ReasonRaw = reasonRaw, Type = type, StartMs = ms };
        _gcs.Add(g);
        _open[count] = g;
    }

    public void OnGcEnd(int count, int generation, double ms)
    {
        if (!_open.Remove(count, out var g)) return;
        g.EndMs = ms;
        _lastEnded = g;
    }

    /// <summary>GCHeapStats follows the GCEnd of the collection it describes.</summary>
    public void OnHeapStats(double ms, ulong gen0, ulong gen1, ulong gen2, ulong loh, ulong? poh, ulong promoted)
    {
        var total = gen0 + gen1 + gen2 + loh + (poh ?? 0);
        _heap.Add(new HeapPointDto(ToNs(ms), total, gen0, gen1, gen2, loh, poh));
        if (_lastEnded is { HeapAfter: null } g)
        {
            g.HeapAfter = total;
            g.GenAfter = new GenSizesDto(gen0, gen1, gen2, loh, poh);
            g.Promoted = promoted;
        }
    }

    public void OnAllocTick(double ms, long bytes)
    {
        if (ms < 0) return;
        var i = (int)(ms / BucketMs);
        while (_alloc.Count <= i) _alloc.Add(0);
        _alloc[i] += bytes;
    }

    /// <summary>Maps a .NET GC reason name onto the common vocabulary; the raw name is kept separately.</summary>
    public static (string Reason, bool Induced) NormalizeReason(string raw)
    {
        if (raw.StartsWith("Induced", StringComparison.Ordinal)) return ("induced", true);
        if (raw.Contains("LowMemory", StringComparison.Ordinal)) return ("low-memory", false);
        return raw switch
        {
            "AllocSmall" => ("allocation", false),
            "AllocLarge" or "OutOfSpaceLOH" => ("large-object", false),
            _ => ("other", false),
        };
    }

    private static string KindOf(string type) => type switch
    {
        "NonConcurrentGC" or "ForegroundGC" => "blocking",
        "BackgroundGC" => "background",
        _ => "other",
    };

    private static ulong ToNs(double ms) => (ulong)Math.Max(0, Math.Round(ms * 1_000_000.0));

    public byte[] BuildJson(long startUnixNs, long durationNs)
    {
        var finished = _gcs.Where(g => g.EndMs.HasValue).ToList();

        // A pause belongs to the GC whose window contains its start; otherwise to the first
        // GC that starts inside it (a blocking GC's suspension precedes its GCStart).
        foreach (var (ps, pe) in _pauses)
        {
            var owner = finished.FirstOrDefault(g => g.StartMs <= ps && ps <= g.EndMs!.Value)
                        ?? finished.Where(g => g.StartMs >= ps && g.StartMs <= pe).OrderBy(g => g.StartMs).FirstOrDefault();
            if (owner is null) continue;
            owner.PauseMs += pe - ps;
            owner.FirstPauseStartMs = Math.Min(owner.FirstPauseStartMs, ps);
            owner.LastPauseEndMs = Math.Max(owner.LastPauseEndMs, pe);
        }

        var events = new List<GcEventDto>();
        foreach (var g in finished.OrderBy(g => Math.Min(g.StartMs, g.FirstPauseStartMs)))
        {
            // The event spans from the earliest of GC start / its pauses to the latest of GC end / its pauses.
            var start = Math.Min(g.StartMs, g.FirstPauseStartMs);
            var end = Math.Max(g.EndMs!.Value, g.LastPauseEndMs);
            var (reason, induced) = NormalizeReason(g.ReasonRaw);
            ulong? before = events.Count > 0 ? events[^1].HeapAfterBytes : null;
            events.Add(new GcEventDto(
                events.Count + 1, ToNs(start), g.Generation, KindOf(g.Type), reason, g.ReasonRaw, induced,
                Math.Min(ToNs(g.PauseMs), ToNs(end - start)), ToNs(end - start),
                before, g.HeapAfter, g.GenAfter, g.Promoted));
        }

        var buckets = (int)Math.Ceiling(durationNs / (BucketMs * 1_000_000.0));
        var alloc = new List<long>(_alloc);
        while (alloc.Count < buckets) alloc.Add(0);

        var data = new GcDataDto(
            1, "dotnet", startUnixNs, (ulong)Math.Max(0, durationNs), "native", events, _heap,
            new AllocSeriesDto(1_000_000_000UL, alloc),
            new List<string> { "allocation rate is derived from allocation ticks (about 100 KB apart) and is approximate" });
        return JsonSerializer.SerializeToUtf8Bytes(data, JsonOptions);
    }

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = JsonIgnoreCondition.WhenWritingNull,
    };

    private sealed record GenSizesDto(ulong Gen0, ulong Gen1, ulong Gen2, ulong Loh, ulong? Poh);

    private sealed record GcEventDto(
        int Id, ulong StartNs, int? Generation, string Kind, string Reason, string? ReasonRaw, bool Induced,
        ulong PauseNs, ulong DurationNs, ulong? HeapBeforeBytes, ulong? HeapAfterBytes, GenSizesDto? GenAfter, ulong? PromotedBytes);

    private sealed record HeapPointDto(ulong TNs, ulong TotalBytes, ulong? Gen0, ulong? Gen1, ulong? Gen2, ulong? Loh, ulong? Poh);

    private sealed record AllocSeriesDto(ulong BucketNs, List<long> Bytes);

    private sealed record GcDataDto(
        int Schema, string Runtime, long StartUnixNs, ulong DurationNs, string Fidelity,
        List<GcEventDto> Events, List<HeapPointDto> Heap, AllocSeriesDto Alloc, List<string> Notes);
}
