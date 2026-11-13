using System.Diagnostics;
using Microsoft.Diagnostics.NETCore.Client;

namespace PrismDotnet;

public static class ProcessList
{
    public static void List(Protocol protocol)
    {
        foreach (var pid in DiagnosticsClient.GetPublishedProcesses().OrderBy(p => p))
        {
            string name = "";
            try { name = Process.GetProcessById(pid).ProcessName; } catch { }
            // The client library does not expose the command line or runtime version publicly.
            protocol.Process(pid, name, null, null);
        }
    }

    /// <summary>Throws a <see cref="HelperException"/> naming why <paramref name="pid"/> cannot be profiled.</summary>
    public static void EnsureAttachable(int pid)
    {
        Process proc;
        try { proc = Process.GetProcessById(pid); }
        catch (ArgumentException) { throw new HelperException("not_found", $"no process with pid {pid}"); }
        if (proc.HasExited) throw new HelperException("not_found", $"process {pid} has exited");
        if (DiagnosticsClient.GetPublishedProcesses().Contains(pid)) return;

        switch (HasRuntime(proc))
        {
            case true:
                throw new HelperException("diagnostics_disabled",
                    $"pid {pid} runs .NET but its diagnostics endpoint is not reachable: DOTNET_EnableDiagnostics=0 may be set, or it runs as another user / with a different TMPDIR");
            case false:
                throw new HelperException("not_dotnet", $"pid {pid} is not a .NET process");
            default:
                throw new HelperException("permission",
                    $"cannot inspect pid {pid}: it runs as another user, or access was denied");
        }
    }

    /// <summary>True/false if known, null if the process could not be inspected.</summary>
    private static bool? HasRuntime(Process proc)
    {
        try
        {
            if (OperatingSystem.IsLinux())
                return File.ReadAllText($"/proc/{proc.Id}/maps").Contains("libcoreclr.so");
            if (OperatingSystem.IsWindows())
                return proc.Modules.Cast<ProcessModule>()
                    .Any(m => m.ModuleName.StartsWith("coreclr", StringComparison.OrdinalIgnoreCase));
            return false;
        }
        catch (Exception e) when (e is UnauthorizedAccessException or IOException or System.ComponentModel.Win32Exception or InvalidOperationException)
        {
            return null;
        }
    }
}
