using System.Text.Json;

namespace PerfhawkDotnet;

/// <summary>A failure that maps to a protocol <c>error</c> event.</summary>
public sealed class HelperException : Exception
{
    public string Code { get; }

    public HelperException(string code, string message) : base(message) => Code = code;
}

/// <summary>JSON-lines protocol, version 1: one JSON object per line on stdout.</summary>
public sealed class Protocol
{
    public const int Version = 1;
    public const string HelperVersion = "0.1.0";

    private static readonly JsonSerializerOptions Options = new()
    {
        PropertyNamingPolicy = JsonNamingPolicy.SnakeCaseLower,
        DefaultIgnoreCondition = System.Text.Json.Serialization.JsonIgnoreCondition.WhenWritingNull,
    };

    private readonly TextWriter _out;
    private readonly object _gate = new();

    public Protocol(TextWriter output) => _out = output;

    private void Emit(object evt)
    {
        var line = JsonSerializer.Serialize(evt, Options);
        lock (_gate)
        {
            _out.Write(line);
            _out.Write('\n');
            _out.Flush();
        }
    }

    public void Hello() => Emit(new { type = "hello", protocol = Version, helper_version = HelperVersion });

    public void Progress(string message) => Emit(new { type = "progress", message });

    public void Warning(string message) => Emit(new { type = "warning", message });

    public void Artifact(string kind, string path, int samples) =>
        Emit(new { type = "artifact", kind, path, samples });

    public void Process(int pid, string name, string? commandLine, string? runtimeVersion) =>
        Emit(new { type = "process", pid, name, command_line = commandLine, runtime_version = runtimeVersion });

    public void Error(string code, string message) => Emit(new { type = "error", code, message });

    public void Done(string end) => Emit(new { type = "done", end });
}
