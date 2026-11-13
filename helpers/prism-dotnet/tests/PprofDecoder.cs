using System.IO.Compression;
using System.Text;

namespace PrismDotnet.Tests;

/// <summary>A deliberately independent, minimal pprof reader used to verify PprofWriter.</summary>
public sealed class DecodedProfile
{
    public List<string> Strings { get; } = new();
    public List<(string Type, string Unit)> SampleTypes { get; } = new();
    public List<DecodedSample> Samples { get; } = new();
    public Dictionary<ulong, ulong> LocationFunction { get; } = new();
    public Dictionary<ulong, (string Name, string File)> Functions { get; } = new();
    public string? DefaultSampleType { get; set; }
    public long DurationNanos { get; set; }

    public IEnumerable<string> FramesOf(DecodedSample s) =>
        s.LocationIds.Select(l => Functions[LocationFunction[l]].Name);
}

public sealed class DecodedSample
{
    public List<ulong> LocationIds { get; } = new();
    public List<long> Values { get; } = new();
    public List<(string Key, string Str)> Labels { get; } = new();
}

public static class PprofDecoder
{
    public static DecodedProfile DecodeGzip(string path)
    {
        using var fs = File.OpenRead(path);
        using var gz = new GZipStream(fs, CompressionMode.Decompress);
        using var ms = new MemoryStream();
        gz.CopyTo(ms);
        return Decode(ms.ToArray());
    }

    public static DecodedProfile DecodeGzip(byte[] gz)
    {
        using var gzs = new GZipStream(new MemoryStream(gz), CompressionMode.Decompress);
        using var ms = new MemoryStream();
        gzs.CopyTo(ms);
        return Decode(ms.ToArray());
    }

    private readonly record struct Field(int Number, int WireType, ulong Varint, byte[] Bytes);

    private static ulong ReadVarint(byte[] b, ref int i)
    {
        ulong v = 0;
        int shift = 0;
        while (true)
        {
            byte x = b[i++];
            v |= (ulong)(x & 0x7f) << shift;
            if ((x & 0x80) == 0) return v;
            shift += 7;
        }
    }

    private static List<Field> Fields(byte[] b)
    {
        var list = new List<Field>();
        int i = 0;
        while (i < b.Length)
        {
            ulong key = ReadVarint(b, ref i);
            int num = (int)(key >> 3), wt = (int)(key & 7);
            switch (wt)
            {
                case 0: list.Add(new Field(num, wt, ReadVarint(b, ref i), Array.Empty<byte>())); break;
                case 2:
                    int len = (int)ReadVarint(b, ref i);
                    list.Add(new Field(num, wt, 0, b[i..(i + len)]));
                    i += len;
                    break;
                default: throw new InvalidDataException($"wire type {wt}");
            }
        }
        return list;
    }

    private static List<ulong> Packed(byte[] b)
    {
        var l = new List<ulong>();
        int i = 0;
        while (i < b.Length) l.Add(ReadVarint(b, ref i));
        return l;
    }

    public static DecodedProfile Decode(byte[] raw)
    {
        var p = new DecodedProfile();
        var top = Fields(raw);
        foreach (var f in top.Where(f => f.Number == 6))
            p.Strings.Add(Encoding.UTF8.GetString(f.Bytes));
        string S(ulong i) => p.Strings[(int)i];
        foreach (var f in top)
        {
            switch (f.Number)
            {
                case 1:
                    var vt = Fields(f.Bytes);
                    p.SampleTypes.Add((S(vt.First(x => x.Number == 1).Varint), S(vt.First(x => x.Number == 2).Varint)));
                    break;
                case 2:
                    var s = new DecodedSample();
                    foreach (var sf in Fields(f.Bytes))
                    {
                        if (sf.Number == 1) s.LocationIds.AddRange(sf.WireType == 2 ? Packed(sf.Bytes) : new List<ulong> { sf.Varint });
                        else if (sf.Number == 2) s.Values.AddRange((sf.WireType == 2 ? Packed(sf.Bytes) : new List<ulong> { sf.Varint }).Select(v => (long)v));
                        else if (sf.Number == 3)
                        {
                            var lf = Fields(sf.Bytes);
                            s.Labels.Add((S(lf.First(x => x.Number == 1).Varint), S(lf.First(x => x.Number == 2).Varint)));
                        }
                    }
                    p.Samples.Add(s);
                    break;
                case 4:
                    var loc = Fields(f.Bytes);
                    ulong id = loc.First(x => x.Number == 1).Varint;
                    var line = Fields(loc.First(x => x.Number == 4).Bytes);
                    p.LocationFunction[id] = line.First(x => x.Number == 1).Varint;
                    break;
                case 5:
                    var fn = Fields(f.Bytes);
                    ulong fid = fn.First(x => x.Number == 1).Varint;
                    ulong name = fn.First(x => x.Number == 2).Varint;
                    var file = fn.FirstOrDefault(x => x.Number == 4);
                    p.Functions[fid] = (S(name), file.Number == 4 ? S(file.Varint) : "");
                    break;
                case 10: p.DurationNanos = (long)f.Varint; break;
                case 14: p.DefaultSampleType = S(f.Varint); break;
            }
        }
        return p;
    }
}
