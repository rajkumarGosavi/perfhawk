using System.Text.Json;
using PerfhawkDotnet.Convert;
using Xunit;

namespace PerfhawkDotnet.Tests;

public class CounterCollectorTests
{
    private const double Ms = 1_000_000.0;

    private static JsonElement Build(CounterCollector c, long durationMs = 5000)
    {
        using var doc = JsonDocument.Parse(c.BuildJson(1_700_000_000_000_000_000, (long)(durationMs * Ms)));
        return doc.RootElement.Clone();
    }

    private static Dictionary<string, object> Mean(string name, double v) => new()
    {
        ["Name"] = name, ["DisplayName"] = "ThreadPool Queue Length", ["DisplayUnits"] = "",
        ["CounterType"] = "Mean", ["Mean"] = v,
    };

    private static Dictionary<string, object> Sum(string name, double inc) => new()
    {
        ["Name"] = name, ["DisplayName"] = "Exception Count", ["DisplayUnits"] = "",
        ["CounterType"] = "Sum", ["Increment"] = inc,
    };

    [Fact]
    public void MeanCounterBecomesGauge()
    {
        var c = new CounterCollector();
        c.OnCounterPayload(1000, Mean("threadpool-queue-length", 3));
        c.OnCounterPayload(2000, Mean("threadpool-queue-length", 7));
        var s = Build(c).GetProperty("series")[0];
        Assert.Equal("threadpool-queue-length", s.GetProperty("name").GetString());
        Assert.Equal("ThreadPool Queue Length", s.GetProperty("display_name").GetString());
        Assert.Equal("gauge", s.GetProperty("kind").GetString());
        var p = s.GetProperty("points");
        Assert.Equal(2, p.GetArrayLength());
        Assert.Equal(1000 * Ms, p[0].GetProperty("t_ns").GetDouble(), 1);
        Assert.Equal(7, p[1].GetProperty("value").GetDouble());
    }

    [Fact]
    public void SumCounterBecomesRate()
    {
        var c = new CounterCollector();
        c.OnCounterPayload(1000, Sum("exception-count", 4));
        var s = Build(c).GetProperty("series")[0];
        Assert.Equal("rate", s.GetProperty("kind").GetString());
        Assert.Equal(4, s.GetProperty("points")[0].GetProperty("value").GetDouble());
    }

    [Fact]
    public void NonFiniteValueIsDropped()
    {
        var c = new CounterCollector();
        c.OnCounterPayload(1000, Mean("cpu-usage", double.NaN));
        c.OnCounterPayload(2000, Mean("cpu-usage", double.PositiveInfinity));
        c.OnCounterPayload(3000, Mean("cpu-usage", 12.5));
        var p = Build(c).GetProperty("series")[0].GetProperty("points");
        Assert.Equal(1, p.GetArrayLength());
        Assert.Equal(12.5, p[0].GetProperty("value").GetDouble());
    }

    [Fact]
    public void NonAscendingTimeIsDropped()
    {
        var c = new CounterCollector();
        c.OnCounterPayload(1000, Mean("cpu-usage", 1));
        c.OnCounterPayload(1000, Mean("cpu-usage", 2));
        c.OnCounterPayload(900, Mean("cpu-usage", 3));
        Assert.Equal(1, Build(c).GetProperty("series")[0].GetProperty("points").GetArrayLength());
    }

    [Fact]
    public void PointCapIsRespected()
    {
        var c = new CounterCollector();
        for (int i = 0; i < CounterCollector.MaxPoints + 10; i++)
            c.OnCounterPayload(i * 1000, Mean("cpu-usage", i));
        Assert.Equal(CounterCollector.MaxPoints, Build(c).GetProperty("series")[0].GetProperty("points").GetArrayLength());
    }

    [Fact]
    public void StarvationAddsEvent()
    {
        var c = new CounterCollector();
        c.OnStarvation(1500, 12);
        var e = Build(c).GetProperty("events")[0];
        Assert.Equal("threadpool_starvation", e.GetProperty("kind").GetString());
        Assert.Equal("worker threads -> 12", e.GetProperty("detail").GetString());
        Assert.Equal(1500 * Ms, e.GetProperty("t_ns").GetDouble(), 1);
        Assert.Equal(1, c.EventCount);
    }

    [Fact]
    public void EmptyCollectorWritesValidJson()
    {
        var root = Build(new CounterCollector());
        Assert.Equal(1, root.GetProperty("schema").GetInt32());
        Assert.Equal("dotnet", root.GetProperty("runtime").GetString());
        Assert.Equal(1_000_000_000UL, root.GetProperty("interval_ns").GetUInt64());
        Assert.Equal(0, root.GetProperty("series").GetArrayLength());
        Assert.Equal(0, root.GetProperty("events").GetArrayLength());
    }
}
