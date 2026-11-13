using System.IO.Compression;
using System.Text;

namespace PerfhawkDotnet.Pprof;

public readonly record struct Frame(string Name, string File);

/// <summary>
/// Builds a pprof <c>Profile</c> message with a hand-written protobuf encoder
/// (varints, length-delimited fields, packed repeated), so the helper needs no
/// protobuf package.
/// </summary>
public sealed class PprofBuilder
{
    private readonly List<string> _strings = new() { "" };
    private readonly Dictionary<string, int> _stringIndex = new() { [""] = 0 };
    private readonly List<(int Type, int Unit)> _types = new();
    private readonly Dictionary<(string, string), ulong> _functionIds = new();
    private readonly List<(ulong Id, int Name, int File)> _functions = new();
    private readonly List<(ulong[] Locations, long[] Values, (int Key, int Str)? Label)> _samples = new();
    // Identical (stack, label) samples are merged by summing their values.
    private readonly Dictionary<string, int> _sampleIndex = new();
    private int _defaultType = -1;
    private long _timeNanos;
    private long _durationNanos;
    private (int Type, int Unit, long Period)? _period;

    public PprofBuilder(params (string Type, string Unit)[] sampleTypes)
    {
        foreach (var (t, u) in sampleTypes) _types.Add((Intern(t), Intern(u)));
    }

    public int SampleCount => _samples.Count;

    private int Intern(string s)
    {
        if (_stringIndex.TryGetValue(s, out var i)) return i;
        i = _strings.Count;
        _strings.Add(s);
        _stringIndex[s] = i;
        return i;
    }

    public void SetDefaultSampleType(string name) => _defaultType = Intern(name);

    public void SetDuration(long timeNanos, long durationNanos)
    {
        _timeNanos = timeNanos;
        _durationNanos = durationNanos;
    }

    public void SetPeriod(string type, string unit, long period) => _period = (Intern(type), Intern(unit), period);

    /// <summary>Adds a sample. <paramref name="rootFirst"/> is root-first; pprof stores leaf-first.</summary>
    public void AddSample(IReadOnlyList<Frame> rootFirst, long[] values, string? labelKey = null, string? labelStr = null)
    {
        if (values.Length != _types.Count)
            throw new ArgumentException($"expected {_types.Count} values, got {values.Length}", nameof(values));
        var locs = new ulong[rootFirst.Count];
        for (int i = 0; i < rootFirst.Count; i++)
            locs[rootFirst.Count - 1 - i] = FunctionId(rootFirst[i]);
        (int, int)? label = labelKey is null ? null : (Intern(labelKey), Intern(labelStr ?? ""));
        var key = string.Join(',', locs) + (label is { } l ? $"|{l.Item1}:{l.Item2}" : "");
        if (_sampleIndex.TryGetValue(key, out var existing))
        {
            var sum = _samples[existing].Values;
            for (int i = 0; i < sum.Length; i++) sum[i] += values[i];
            return;
        }
        _sampleIndex[key] = _samples.Count;
        _samples.Add((locs, (long[])values.Clone(), label));
    }

    // Location ids equal function ids: one location per function.
    private ulong FunctionId(Frame f)
    {
        if (_functionIds.TryGetValue((f.Name, f.File), out var id)) return id;
        id = (ulong)_functions.Count + 1;
        _functionIds[(f.Name, f.File)] = id;
        _functions.Add((id, Intern(f.Name), Intern(f.File)));
        return id;
    }

    public byte[] ToBytes()
    {
        var w = new ProtoWriter();
        foreach (var (t, u) in _types)
            w.Message(1, m => { m.Varint(1, (ulong)t); m.Varint(2, (ulong)u); });
        foreach (var (locs, values, label) in _samples)
            w.Message(2, m =>
            {
                m.Packed(1, locs);
                m.Packed(2, values.Select(v => (ulong)v));
                if (label is { } l) m.Message(3, lm => { lm.Varint(1, (ulong)l.Key); lm.Varint(2, (ulong)l.Str); });
            });
        foreach (var (id, _, _) in _functions)
            w.Message(4, m =>
            {
                m.Varint(1, id);
                m.Message(4, line => { line.Varint(1, id); });
            });
        foreach (var (id, name, file) in _functions)
            w.Message(5, m =>
            {
                m.Varint(1, id);
                m.Varint(2, (ulong)name);
                m.Varint(3, (ulong)name);
                if (file != 0) m.Varint(4, (ulong)file);
            });
        foreach (var s in _strings) w.Bytes(6, Encoding.UTF8.GetBytes(s));
        if (_timeNanos != 0) w.Varint(9, (ulong)_timeNanos);
        if (_durationNanos != 0) w.Varint(10, (ulong)_durationNanos);
        if (_period is { } p)
        {
            w.Message(11, m => { m.Varint(1, (ulong)p.Type); m.Varint(2, (ulong)p.Unit); });
            w.Varint(12, (ulong)p.Period);
        }
        if (_defaultType >= 0) w.Varint(14, (ulong)_defaultType);
        return w.ToArray();
    }

    public byte[] ToGzip()
    {
        var raw = ToBytes();
        using var ms = new MemoryStream();
        using (var gz = new GZipStream(ms, CompressionLevel.Fastest, leaveOpen: true))
            gz.Write(raw, 0, raw.Length);
        return ms.ToArray();
    }
}

internal sealed class ProtoWriter
{
    private readonly MemoryStream _ms = new();

    public byte[] ToArray() => _ms.ToArray();

    private void RawVarint(ulong v)
    {
        while (v >= 0x80)
        {
            _ms.WriteByte((byte)(v | 0x80));
            v >>= 7;
        }
        _ms.WriteByte((byte)v);
    }

    private void Tag(int field, int wireType) => RawVarint(((ulong)field << 3) | (uint)wireType);

    public void Varint(int field, ulong v)
    {
        Tag(field, 0);
        RawVarint(v);
    }

    public void Bytes(int field, byte[] data)
    {
        Tag(field, 2);
        RawVarint((ulong)data.Length);
        _ms.Write(data, 0, data.Length);
    }

    public void Message(int field, Action<ProtoWriter> build)
    {
        var inner = new ProtoWriter();
        build(inner);
        Bytes(field, inner.ToArray());
    }

    public void Packed(int field, IEnumerable<ulong> values)
    {
        var inner = new ProtoWriter();
        foreach (var v in values) inner.RawVarint(v);
        Bytes(field, inner.ToArray());
    }
}
