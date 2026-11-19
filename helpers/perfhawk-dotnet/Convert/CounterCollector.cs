using System.Text.Json;

namespace PerfhawkDotnet.Convert;

/// <summary>
/// Turns <c>System.Runtime</c> EventCounters payloads and threadpool starvation
/// events into Perfhawk's runtime-neutral counter data (<c>counters.json</c>).
/// Every counter the provider emits is kept; nothing is hard-coded.
/// </summary>
public sealed class CounterCollector
{
    /// <summary>Matches the Rust model's caps so a long trace never makes the whole file invalid.</summary>
    public const int MaxSeries = 256;
    public const int MaxPoints = 700;
    private const ulong IntervalNs = 1_000_000_000UL;

    private sealed class Series
    {
        public required string Name { get; init; }
        public required string DisplayName { get; init; }
        public required string Unit { get; init; }
        public required string Kind { get; init; }
        public List<PointDto> Points { get; } = new();
    }

    private readonly List<Series> _series = new();
    private readonly Dictionary<string, Series> _byName = new();
    private readonly List<EventDto> _events = new();

    public int EventCount { get; private set; }

    private static string? Str(IDictionary<string, object> p, string key) =>
        p.TryGetValue(key, out var v) ? v?.ToString() : null;

    private static double? Num(IDictionary<string, object> p, string key)
    {
        if (!p.TryGetValue(key, out var v) || v is null) return null;
        try { return System.Convert.ToDouble(v, System.Globalization.CultureInfo.InvariantCulture); }
        catch (Exception e) when (e is FormatException or InvalidCastException or OverflowException) { return null; }
    }

    /// <param name="ms">Milliseconds since the trace started.</param>
    /// <param name="payload">The <c>Payload</c> dictionary of an <c>EventCounters</c> event.</param>
    public void OnCounterPayload(double ms, IDictionary<string, object> payload)
    {
        var name = Str(payload, "Name");
        if (string.IsNullOrEmpty(name)) return;
        bool isSum = Str(payload, "CounterType") == "Sum";
        var value = Num(payload, isSum ? "Increment" : "Mean");
        if (value is not double v || !double.IsFinite(v)) return;

        if (!_byName.TryGetValue(name, out var s))
        {
            if (_series.Count >= MaxSeries) return;
            var display = Str(payload, "DisplayName");
            s = new Series
            {
                Name = name,
                DisplayName = string.IsNullOrEmpty(display) ? name : display,
                Unit = Str(payload, "DisplayUnits") ?? "",
                Kind = isSum ? "rate" : "gauge",
            };
            _byName[name] = s;
            _series.Add(s);
        }
        ulong t = (ulong)Math.Max(0, ms * 1_000_000.0);
        if (s.Points.Count >= MaxPoints) return;
        if (s.Points.Count > 0 && s.Points[^1].TNs >= t) return;
        s.Points.Add(new PointDto(t, v));
        EventCount++;
    }

    public void OnStarvation(double ms, int newWorkerCount)
    {
        _events.Add(new EventDto((ulong)Math.Max(0, ms * 1_000_000.0), "threadpool_starvation", $"worker threads -> {newWorkerCount}"));
        EventCount++;
    }

    public byte[] BuildJson(long startUnixNs, long durationNs)
    {
        var series = _series.Select(s => new SeriesDto(s.Name, s.DisplayName, s.Unit, s.Kind, s.Points)).ToList();
        var data = new DataDto(1, "dotnet", startUnixNs, (ulong)Math.Max(0, durationNs), IntervalNs, series, _events, new List<string>());
        return JsonSerializer.SerializeToUtf8Bytes(data, JsonOptions);
    }

    private static readonly JsonSerializerOptions JsonOptions = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
    };

    private sealed record PointDto(ulong TNs, double Value);
    private sealed record EventDto(ulong TNs, string Kind, string Detail);
    private sealed record SeriesDto(string Name, string DisplayName, string Unit, string Kind, List<PointDto> Points);
    private sealed record DataDto(
        int Schema, string Runtime, long StartUnixNs, ulong DurationNs, ulong IntervalNs,
        List<SeriesDto> Series, List<EventDto> Events, List<string> Notes);
}
