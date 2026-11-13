# Troubleshooting

When reporting a problem, include `perfhawk --version`, your Windows version, the exact command and its full output.
The [user guide](USER_GUIDE.md#8-troubleshooting) has more command-by-command detail.

## Installing and starting

### "Windows protected your PC" or the file was deleted by antivirus

Perfhawk reads other processes' memory and diagnostics channels to profile them, and new unsigned programs often trigger SmartScreen or antivirus heuristics.

1. Verify the download against `SHA256SUMS` (see [INSTALL.md](INSTALL.md#option-2-manual-zip)). If it matches, the file is what we published.
2. SmartScreen: click **More info**, then **Run anyway**.
3. Antivirus: restore the file from quarantine and add the install folder as an exclusion, or report the false positive to your vendor.

### `perfhawk` is not recognised

Open a **new** terminal after installing. If it still fails, check that the install folder is on your user `PATH`:

```powershell
$env:Path -split ';' | Select-String perfhawk
```

or run `& "$env:LOCALAPPDATA\Programs\perfhawk\perfhawk.exe" --version`.

### `cannot bind 127.0.0.1:7878`

Something else (often another `perfhawk ui`) uses the port. Choose another: `perfhawk ui --port 7900`, or `--port 0` for any free port.

### The browser shows "UI not built"

You built from source without building the UI first. Run `npm ci; npm run build` in `ui\` and rebuild. Release packages are not affected.

### `403 forbidden` in the browser

Open the exact address Perfhawk printed (`http://127.0.0.1:<port>/`). Perfhawk rejects other host names (including a machine name or a proxy) on purpose, to block DNS-rebinding attacks.

## Recording

First, see what Perfhawk can see:

```powershell
perfhawk targets
```

### Nothing is listed / "no targets found"

| Target | Cause | Fix |
|---|---|---|
| .NET | Not .NET 8 or newer | Only .NET 8+ is supported |
| .NET | `DOTNET_EnableDiagnostics=0` is set for the program | Restart it without that variable |
| .NET | The program runs as another user or elevated | Run Perfhawk in the same user context (elevated terminal for an elevated program) |
| Go | The program does not run the Perfhawk agent | Add the agent, or use `--url` against `net/http/pprof`, or `--go-test` (see [Quickstart](QUICKSTART.md#profile-your-own-go-program)) |
| Go | Started with `go run .` | `go run` starts a child; build with `go build` and let Perfhawk launch the `.exe` |

### `the .NET helper (perfhawk-dotnet) was not found`

`perfhawk-dotnet.exe` must sit next to `perfhawk.exe`. Reinstall Perfhawk, copy it back, or set `PERFHAWK_DOTNET` to its path.

### `perfhawk-dotnet protocol X is not supported`

`perfhawk.exe` and `perfhawk-dotnet.exe` come from different versions. Reinstall so both come from the same release.

### "Access is denied" when attaching

The target runs as a different user or with higher privileges. Start Perfhawk in an elevated terminal for an elevated target, or profile a process you own.

### `program did not start the perfhawk agent`

The Go program you launched does not import the agent. Add `import _ "github.com/rajkumarGosavi/perfhawk/agents/go"` and rebuild, or use `--url` / `--go-test`.

### The session is `partial`

The target exited before the window ended. Perfhawk keeps what it captured. Increase the workload or shorten `--duration`.

### A profile is empty, or has very few samples

- The program was idle during the window. Drive load while recording.
- Go mutex and block profiles stay empty unless sampling is enabled. When recording through the Perfhawk agent this is handled for you.
- A very short `--duration` yields few samples; use 10-30 seconds under load.

### Ctrl+C during a recording

Perfhawk stops, keeps what was captured as a `cancelled` session, and the Go agent restores any sampling rates it changed.

## Importing and reading profiles

| Symptom | Fix |
|---|---|
| `unknown format` on import | The file is not pprof, collapsed or speedscope. Check it is not truncated. |
| Profile kind shows `unknown` | Collapsed and speedscope files carry no kind. Re-import with `--kind cpu` or `--kind wall` (use a fresh `--home` or `perfhawk rm` first; identical files are de-duplicated). |
| `*.pb.gz` is not expanded | PowerShell does not expand wildcards for native programs. Use `perfhawk import (Get-ChildItem *.pb.gz)`. |
| `is ambiguous (2 matches)` | Several profiles share that name. Use the short id from `perfhawk ls`. |
| Numbers differ from `dotnet-trace` or PerfView | .NET CPU values come from sampling and are marked *approximate*; see the fidelity badge and [limitations](LIMITATIONS.md). |

## Data and disk

Profiles live in Perfhawk's data folder (`perfhawk.db` plus a `blobs\` folder). Show or change it:

```powershell
perfhawk --home D:\perfhawk-data ls          # use a different folder for one command
$env:PERFHAWK_HOME = "D:\perfhawk-data"      # ...or for the whole session
```

Delete the folder to remove everything, or use `perfhawk rm <id>`.

## Still stuck?

Open an issue at <https://github.com/rajkumarGosavi/perfhawk/issues> with the details above. Do not post profiles that contain code or data you cannot share.
