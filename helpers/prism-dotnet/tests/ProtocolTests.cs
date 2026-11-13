using System.Text.Json;
using Xunit;

namespace PrismDotnet.Tests;

public class ProtocolTests
{
    [Fact]
    public async Task FirstLineIsHello()
    {
        var sw = new StringWriter();
        var exit = await Program.RunAsync(new[] { "list" }, sw, new StringReader(""), CancellationToken.None);
        Assert.Equal(0, exit);
        var first = sw.ToString().Split('\n')[0];
        var doc = JsonDocument.Parse(first).RootElement;
        Assert.Equal("hello", doc.GetProperty("type").GetString());
        Assert.Equal(1, doc.GetProperty("protocol").GetInt32());
        Assert.False(string.IsNullOrEmpty(doc.GetProperty("helper_version").GetString()));
        Assert.Equal("{\"type\":\"hello\"", first[..15]);
    }

    [Fact]
    public async Task UnknownCommandIsAnInternalError()
    {
        var sw = new StringWriter();
        var exit = await Program.RunAsync(new[] { "frobnicate" }, sw, new StringReader(""), CancellationToken.None);
        Assert.Equal(1, exit);
        var lines = sw.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries);
        Assert.Equal("hello", JsonDocument.Parse(lines[0]).RootElement.GetProperty("type").GetString());
        var err = JsonDocument.Parse(lines[^1]).RootElement;
        Assert.Equal("error", err.GetProperty("type").GetString());
        Assert.Equal("internal", err.GetProperty("code").GetString());
    }

    [Fact]
    public async Task ConvertRejectsMissingInput()
    {
        var sw = new StringWriter();
        var exit = await Program.RunAsync(new[] { "convert", "--in", "does-not-exist.nettrace", "--out", Path.GetTempPath(), "--kinds", "wall" }, sw, new StringReader(""), CancellationToken.None);
        Assert.Equal(1, exit);
        Assert.Contains("\"type\":\"error\"", sw.ToString());
    }
}
