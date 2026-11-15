using System.Text;
using Xunit;

namespace PerfhawkDotnet.Tests;

public class HeapGraphBuilderTests
{
    private sealed record Decoded(
        bool Complete, string Runtime, string[] Types, uint[] NodeType, ulong[] Size,
        uint[] EdgeCounts, uint[] Targets, (uint Node, string Kind)[] Roots);

    private static Decoded Decode(byte[] b)
    {
        using var r = new BinaryReader(new MemoryStream(b), Encoding.UTF8);
        Assert.Equal("PHHP", Encoding.ASCII.GetString(r.ReadBytes(4)));
        Assert.Equal(1u, r.ReadUInt32());
        bool complete = r.ReadByte() != 0;
        r.ReadInt64();
        string Str() => Encoding.UTF8.GetString(r.ReadBytes((int)r.ReadUInt32()));
        string runtime = Str();
        var types = Enumerable.Range(0, (int)r.ReadUInt32()).Select(_ => Str()).ToArray();
        int n = (int)r.ReadUInt32();
        var nodeType = Enumerable.Range(0, n).Select(_ => r.ReadUInt32()).ToArray();
        var size = Enumerable.Range(0, n).Select(_ => r.ReadUInt64()).ToArray();
        var counts = Enumerable.Range(0, n).Select(_ => r.ReadUInt32()).ToArray();
        var targets = Enumerable.Range(0, (int)r.ReadUInt32()).Select(_ => r.ReadUInt32()).ToArray();
        var roots = Enumerable.Range(0, (int)r.ReadUInt32()).Select(_ => (r.ReadUInt32(), Str())).ToArray();
        return new Decoded(complete, runtime, types, nodeType, size, counts, targets, roots);
    }

    [Fact]
    public void ResolvesAddressesToNodeIndices()
    {
        var g = new HeapGraphBuilder();
        g.AddType(10, "Cache");
        g.AddType(20, "Item");
        g.AddNode(0x1000, 32, 10, 2);  // node 0 -> 1, 2
        g.AddNode(0x2000, 100, 20, 0); // node 1
        g.AddNode(0x3000, 100, 20, 1); // node 2 -> 0 (cycle)
        g.AddEdge(0x2000);
        g.AddEdge(0x3000);
        g.AddEdge(0x1000);
        g.AddRoot(0x1000, "Handle");

        var d = Decode(g.Build("dotnet", 7));

        Assert.True(d.Complete);
        Assert.Equal(new[] { "Cache", "Item" }, d.Types);
        Assert.Equal(new uint[] { 0, 1, 1 }, d.NodeType);
        Assert.Equal(new uint[] { 2, 0, 1 }, d.EdgeCounts);
        Assert.Equal(new uint[] { 1, 2, 0 }, d.Targets);
        Assert.Equal(new[] { (0u, "Handle") }, d.Roots);
    }

    [Fact]
    public void MissingEdgeTargetOrEdgesMarkTheSnapshotIncomplete()
    {
        var g = new HeapGraphBuilder();
        g.AddNode(0x1000, 8, 1, 3); // claims 3 edges
        g.AddEdge(0x1000);          // only one arrives, plus...
        g.AddEdge(0x9999);          // ...one to an unknown object
        var d = Decode(g.Build("dotnet", 0));

        Assert.False(d.Complete);
        Assert.Equal(new uint[] { 1 }, d.EdgeCounts);
        Assert.Equal(new uint[] { 0 }, d.Targets);
        Assert.StartsWith("<unknown type", d.Types[0]);
    }
}
