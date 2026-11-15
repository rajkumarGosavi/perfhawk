using System.Diagnostics;
using System.Text;
using Microsoft.Diagnostics.NETCore.Client;
using Microsoft.Diagnostics.Tracing;
using Microsoft.Diagnostics.Tracing.Parsers;

namespace PerfhawkDotnet;

/// <summary>
/// Builds a heap graph from the runtime's GC heap-dump events and writes it in the
/// Perfhawk heap snapshot binary format (see crates/perfhawk-model/src/heap.rs).
/// </summary>
public sealed class HeapGraphBuilder
{
    private readonly Dictionary<ulong, string> _typeNames = new();
    private readonly List<ulong> _addr = new();
    private readonly List<ulong> _size = new();
    private readonly List<ulong> _typeId = new();
    private readonly List<long> _edgeCount = new();
    private readonly List<ulong> _edgeTarget = new();
    private readonly List<(ulong Addr, string Kind)> _roots = new();

    public int NodeCount => _addr.Count;
    public long EdgeEvents => _edgeTarget.Count;

    /// <summary>Set when the stream ended before the runtime finished the dump.</summary>
    public bool Incomplete { get; set; }

    public void AddType(ulong typeId, string name) => _typeNames[typeId] = name;

    public void AddNode(ulong address, ulong size, ulong typeId, long edgeCount)
    {
        _addr.Add(address);
        _size.Add(size);
        _typeId.Add(typeId);
        _edgeCount.Add(edgeCount);
    }

    public void AddEdge(ulong target) => _edgeTarget.Add(target);

    public void AddRoot(ulong address, string kind) => _roots.Add((address, kind));

    /// <summary>Resolves addresses to node indices and encodes the snapshot.</summary>
    public byte[] Build(string runtime, long takenUnixNs)
    {
        int n = _addr.Count;
        var index = new Dictionary<ulong, int>(n);
        for (int i = 0; i < n; i++) index[_addr[i]] = i; // a repeated address keeps its last node

        // Types: dense index in first-use order; unknown ids get a placeholder name.
        var typeIndex = new Dictionary<ulong, uint>();
        var types = new List<string>();
        var nodeType = new uint[n];
        for (int i = 0; i < n; i++)
        {
            if (!typeIndex.TryGetValue(_typeId[i], out var ti))
            {
                ti = (uint)types.Count;
                typeIndex[_typeId[i]] = ti;
                types.Add(_typeNames.TryGetValue(_typeId[i], out var name) && !string.IsNullOrEmpty(name)
                    ? name
                    : $"<unknown type 0x{_typeId[i]:x}>");
            }
            nodeType[i] = ti;
        }

        // Edges arrive in node order: node i owns the next _edgeCount[i] targets.
        var counts = new uint[n];
        var targets = new List<uint>(_edgeTarget.Count);
        long cursor = 0;
        long dropped = 0;
        for (int i = 0; i < n; i++)
        {
            long want = _edgeCount[i];
            for (long k = 0; k < want; k++)
            {
                if (cursor >= _edgeTarget.Count) { dropped += want - k; break; }
                if (index.TryGetValue(_edgeTarget[(int)cursor++], out var to))
                {
                    targets.Add((uint)to);
                    counts[i]++;
                }
                else dropped++;
            }
        }
        if (dropped > 0) Incomplete = true;

        using var ms = new MemoryStream();
        using (var w = new BinaryWriter(ms, new UTF8Encoding(false), leaveOpen: true))
        {
            w.Write(Encoding.ASCII.GetBytes("PHHP"));
            w.Write(1u); // schema
            w.Write((byte)(Incomplete ? 0 : 1));
            w.Write(takenUnixNs);
            WriteStr(w, runtime);
            w.Write((uint)types.Count);
            foreach (var t in types) WriteStr(w, t);
            w.Write((uint)n);
            foreach (var t in nodeType) w.Write(t);
            foreach (var s in _size) w.Write(s);
            foreach (var c in counts) w.Write(c);
            w.Write((uint)targets.Count);
            foreach (var t in targets) w.Write(t);
            var roots = _roots.Where(r => index.ContainsKey(r.Addr)).ToList();
            w.Write((uint)roots.Count);
            foreach (var (addr, kind) in roots)
            {
                w.Write((uint)index[addr]);
                WriteStr(w, kind);
            }
        }
        return ms.ToArray();
    }

    private static void WriteStr(BinaryWriter w, string s)
    {
        var b = Encoding.UTF8.GetBytes(s);
        w.Write((uint)b.Length);
        w.Write(b);
    }
}

public static class Snapshotter
{
    // GC | GCHeapDump | GCHeapCollect | GCHeapAndTypeNames | Type: enabling this makes the
    // runtime run one induced gen2 GC and report every live object and reference.
    private const long HeapSnapshotKeywords = 0x1980001;

    /// <summary>
    /// Takes one heap snapshot of <paramref name="client"/>'s process and writes it to
    /// <paramref name="outPath"/>. Returns the node count. This triggers a full (gen2) GC in the
    /// target, so the process pauses for as long as the heap walk takes.
    /// </summary>
    public static int Capture(DiagnosticsClient client, string outPath, int timeoutMs, CancellationToken stop, Action<string>? progress)
    {
        var graph = new HeapGraphBuilder();
        var providers = new[]
        {
            new EventPipeProvider("Microsoft-Windows-DotNETRuntime", System.Diagnostics.Tracing.EventLevel.Verbose, HeapSnapshotKeywords),
        };

        EventPipeSession session;
        try
        {
            session = client.StartEventPipeSession(providers, requestRundown: false, circularBufferMB: 1024);
        }
        catch (Exception e) when (e is ServerNotAvailableException or UnsupportedCommandException or TimeoutException or IOException or UnauthorizedAccessException)
        {
            throw new HelperException("diagnostics_disabled", $"could not start an EventPipe session: {e.Message}");
        }

        long taken = DateTimeOffset.UtcNow.ToUnixTimeMilliseconds() * 1_000_000;
        bool finished = false;
        using (session)
        {
            progress?.Invoke("taking heap snapshot (this runs a full GC in the target)");
            using var source = new EventPipeEventSource(session.EventStream);
            var clr = new Microsoft.Diagnostics.Tracing.Parsers.ClrTraceEventParser(source);
            clr.TypeBulkType += e =>
            {
                for (int i = 0; i < e.Count; i++) { var v = e.Values(i); graph.AddType(v.TypeID, v.TypeName); }
            };
            clr.GCBulkNode += e =>
            {
                for (int i = 0; i < e.Count; i++) { var v = e.Values(i); graph.AddNode(v.Address, v.Size, v.TypeID, v.EdgeCount); }
            };
            clr.GCBulkEdge += e =>
            {
                for (int i = 0; i < e.Count; i++) graph.AddEdge(e.Values(i).Target);
            };
            clr.GCBulkRootEdge += e =>
            {
                for (int i = 0; i < e.Count; i++) { var v = e.Values(i); graph.AddRoot(v.RootedNodeAddress, v.GCRootKind.ToString()); }
            };
            // The heap dump happens inside the induced GC, so its stop marks the end of the data.
            using var done = new ManualResetEventSlim();
            clr.GCStop += _ =>
            {
                if (graph.NodeCount > 0)
                {
                    finished = true;
                    done.Set(); // never call session.Stop() on this thread: it blocks while we must drain the stream
                }
            };

            // Stops the session on a separate thread: when the dump ended, on timeout, or on cancel.
            var stopper = Task.Run(() =>
            {
                WaitHandle.WaitAny(new[] { done.WaitHandle, stop.WaitHandle }, timeoutMs);
                try { session.Stop(); } catch { /* already gone */ }
            });
            try { source.Process(); }
            catch (Exception) when (graph.NodeCount > 0) { /* stream ends abruptly if the target exits */ }
            finally
            {
                done.Set();
                stopper.Wait(TimeSpan.FromSeconds(10));
            }
        }

        if (graph.NodeCount == 0)
            throw new HelperException("no_heap_data", stop.IsCancellationRequested
                ? "cancelled before the heap snapshot finished"
                : "the runtime sent no heap data (it may have exited, or the snapshot timed out)");
        if (!finished) graph.Incomplete = true;

        var bytes = graph.Build("dotnet", taken);
        Directory.CreateDirectory(Path.GetDirectoryName(Path.GetFullPath(outPath))!);
        File.WriteAllBytes(outPath, bytes);
        return graph.NodeCount;
    }
}
