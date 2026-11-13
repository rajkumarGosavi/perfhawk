using PrismDotnet.Pprof;
using Xunit;

namespace PrismDotnet.Tests;

public class PprofWriterTests
{
    [Fact]
    public void RoundTripsThroughGooglePprofSchema()
    {
        var b = new PprofBuilder(("cpu", "nanoseconds"), ("samples", "count"));
        b.SetDefaultSampleType("cpu");
        b.SetDuration(1_700_000_000_000_000_000L, 2_000_000_000L);
        b.AddSample(new[] { new Frame("main", "app.dll"), new Frame("work", "app.dll") }, new long[] { 5_000_000, 5 });
        b.AddSample(new[] { new Frame("main", "app.dll"), new Frame("idle", "lib.dll") }, new long[] { 1_000_000, 1 }, "thread", "42");
        Assert.Equal(2, b.SampleCount);

        var p = PprofDecoder.DecodeGzip(b.ToGzip());

        Assert.Equal("", p.Strings[0]);
        Assert.Equal(new[] { ("cpu", "nanoseconds"), ("samples", "count") }, p.SampleTypes.ToArray());
        Assert.Equal("cpu", p.DefaultSampleType);
        Assert.Equal(2_000_000_000L, p.DurationNanos);
        Assert.Equal(2, p.Samples.Count);

        // pprof stores locations leaf-first.
        Assert.Equal(new[] { "work", "main" }, p.FramesOf(p.Samples[0]).ToArray());
        Assert.Equal(new long[] { 5_000_000, 5 }, p.Samples[0].Values.ToArray());
        Assert.Equal(new[] { "idle", "main" }, p.FramesOf(p.Samples[1]).ToArray());
        Assert.Equal(("thread", "42"), p.Samples[1].Labels.Single());

        // "main" is a single shared function/location.
        Assert.Equal(3, p.Functions.Count);
        Assert.Equal(3, p.LocationFunction.Count);
        Assert.Equal("app.dll", p.Functions[p.LocationFunction[p.Samples[0].LocationIds[1]]].File);
    }

    [Fact]
    public void EmptyProfileIsValid()
    {
        var b = new PprofBuilder(("wall", "nanoseconds"));
        var p = PprofDecoder.DecodeGzip(b.ToGzip());
        Assert.Empty(p.Samples);
        Assert.Equal(("wall", "nanoseconds"), p.SampleTypes.Single());
    }

    [Fact]
    public void RejectsValueCountMismatch()
    {
        var b = new PprofBuilder(("cpu", "nanoseconds"));
        Assert.Throws<ArgumentException>(() => b.AddSample(new[] { new Frame("a", "") }, new long[] { 1, 2 }));
    }

    [Fact]
    public void IdenticalStacksAreAggregated()
    {
        var b = new PprofBuilder(("cpu", "nanoseconds"), ("samples", "count"));
        var stack = new[] { new Frame("main", "a.dll"), new Frame("work", "a.dll") };
        b.AddSample(stack, new long[] { 1_000_000, 1 });
        b.AddSample(stack, new long[] { 2_000_000, 1 });
        b.AddSample(new[] { new Frame("main", "a.dll") }, new long[] { 5, 5 });
        // same stack but a different label stays separate
        b.AddSample(stack, new long[] { 7, 1 }, "thread", "1");
        Assert.Equal(3, b.SampleCount);
        var p = PprofDecoder.DecodeGzip(b.ToGzip());
        Assert.Equal(3, p.Samples.Count);
        Assert.Equal(new long[] { 3_000_000, 2 }, p.Samples[0].Values.ToArray());
    }
}
