using System.Text;
using Microsoft.Diagnostics.NETCore.Client;
using PrismDotnet.Convert;

namespace PrismDotnet;

public static class Program
{
    public static async Task<int> Main(string[] args)
    {
        var stdout = new StreamWriter(Console.OpenStandardOutput(), new UTF8Encoding(false)) { AutoFlush = true, NewLine = "\n" };
        using var cancel = new CancellationTokenSource();
        Console.CancelKeyPress += (_, e) =>
        {
            e.Cancel = true; // let the capture stop cleanly and convert what it has
            cancel.Cancel();
        };
        return await RunAsync(args, stdout, Console.In, cancel.Token);
    }

    private static readonly string[] KnownKinds = TraceConverter.AllKinds;

    /// <summary>Runs one command. <paramref name="stdin"/> is watched for a <c>stop</c> line or EOF.</summary>
    public static async Task<int> RunAsync(string[] args, TextWriter stdout, TextReader stdin, CancellationToken external)
    {
        var protocol = new Protocol(stdout);
        protocol.Hello();
        using var stop = CancellationTokenSource.CreateLinkedTokenSource(external);
        try
        {
            if (args.Length == 0) throw new HelperException("internal", "usage: prism-dotnet <list|collect|launch|convert> ...");
            var (flags, rest) = ParseArgs(args.Skip(1).ToArray());
            switch (args[0])
            {
                case "list":
                    ProcessList.List(protocol);
                    protocol.Done("complete");
                    return 0;

                case "convert":
                {
                    var input = Required(flags, "in");
                    if (!File.Exists(input)) throw new HelperException("not_found", $"{input} does not exist");
                    var kinds = ParseKinds(flags);
                    var arts = TraceConverter.Convert(input, Required(flags, "out"), kinds, BucketMs(flags));
                    foreach (var a in arts) protocol.Artifact(a.Kind, a.Path, a.Samples);
                    protocol.Done("complete");
                    return 0;
                }

                case "collect":
                {
                    int pid = int.Parse(Required(flags, "pid"));
                    int duration = int.Parse(Required(flags, "duration-ms"));
                    var kinds = ParseKinds(flags);
                    var outDir = Required(flags, "out");
                    ProcessList.EnsureAttachable(pid);
                    WatchStdin(stdin, stop);
                    protocol.Progress($"attaching to pid {pid}");
                    var result = await Collector.CollectAsync(new DiagnosticsClient(pid), duration, kinds, outDir, stop.Token, protocol.Progress, bucketMs: BucketMs(flags));
                    Finish(protocol, result);
                    return 0;
                }

                case "launch":
                {
                    if (rest.Count == 0) throw new HelperException("internal", "launch needs a program after --");
                    int duration = int.Parse(Required(flags, "duration-ms"));
                    var kinds = ParseKinds(flags);
                    var outDir = Required(flags, "out");
                    WatchStdin(stdin, stop);
                    var result = await Launcher.LaunchAsync(rest[0], rest.Skip(1).ToList(), duration, kinds, outDir, stop.Token, protocol.Progress, BucketMs(flags));
                    Finish(protocol, result);
                    return 0;
                }

                default:
                    throw new HelperException("internal", $"unknown command '{args[0]}'");
            }
        }
        catch (HelperException e)
        {
            protocol.Error(e.Code, e.Message);
            return 1;
        }
        catch (Exception e) when (e is FormatException or OverflowException)
        {
            protocol.Error("internal", $"invalid number in arguments: {e.Message}");
            return 1;
        }
        catch (Exception e)
        {
            var detail = Environment.GetEnvironmentVariable("PRISM_DOTNET_DEBUG") == "1" ? e.ToString() : e.Message;
            protocol.Error("internal", detail);
            return 1;
        }
    }

    private static void Finish(Protocol protocol, CollectResult result)
    {
        foreach (var a in result.Artifacts) protocol.Artifact(a.Kind, a.Path, a.Samples);
        protocol.Done(result.End);
    }

    private static void WatchStdin(TextReader stdin, CancellationTokenSource stop)
    {
        // Prism closes stdin or writes "stop" to end a capture early.
        var t = new Thread(() =>
        {
            try
            {
                string? line;
                while ((line = stdin.ReadLine()) != null)
                    if (line.Trim() == "stop") break;
            }
            catch { /* treat as EOF */ }
            try { stop.Cancel(); } catch (ObjectDisposedException) { }
        })
        { IsBackground = true };
        t.Start();
    }

    private static (Dictionary<string, string> Flags, List<string> Tail) ParseArgs(string[] args)
    {
        var flags = new Dictionary<string, string>();
        var rest = new List<string>();
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--")
            {
                rest.AddRange(args.Skip(i + 1));
                break;
            }
            if (args[i].StartsWith("--") && i + 1 < args.Length)
                flags[args[i][2..]] = args[++i];
            else
                throw new HelperException("internal", $"unexpected argument '{args[i]}'");
        }
        return (flags, rest);
    }

    /// <summary>Optional <c>--bucket-ms N</c>: label cpu/wall samples with their N-ms time bucket.</summary>
    private static int BucketMs(Dictionary<string, string> flags) =>
        flags.TryGetValue("bucket-ms", out var v) && int.TryParse(v, out var n) && n > 0 ? n : 0;

    private static string Required(Dictionary<string, string> flags, string name) =>
        flags.TryGetValue(name, out var v) ? v : throw new HelperException("internal", $"missing --{name}");

    private static HashSet<string> ParseKinds(Dictionary<string, string> flags)
    {
        var raw = flags.TryGetValue("kinds", out var k) ? k : string.Join(',', KnownKinds);
        var set = new HashSet<string>(raw.Split(',', StringSplitOptions.RemoveEmptyEntries | StringSplitOptions.TrimEntries));
        foreach (var kind in set)
            if (!KnownKinds.Contains(kind))
                throw new HelperException("internal", $"unknown kind '{kind}' (expected {string.Join(", ", KnownKinds)})");
        return set;
    }
}
