# Quickstart (Windows)

Ten minutes from install to your first finding. All commands are for **PowerShell** (Windows Terminal recommended).

**You need:** Prism installed ([INSTALL.md](INSTALL.md)). For the demos, Go 1.22+ and/or the .NET 8 SDK.
Check your install:

```powershell
prism --version
```

If PowerShell says `prism` is not recognised, open a **new** terminal (the installer changes `PATH`), or run it by full path (`$env:LOCALAPPDATA\Programs\prism\prism.exe`).

## 1. Get a demo to profile

The repo includes two small "order service" programs with deliberate performance problems.

```powershell
git clone https://github.com/rajkumarGosavi/prism
cd prism\examples
```

## 2. Record a Go program

Build the demo, then let Prism launch it and record for 10 seconds:

```powershell
cd go-demo
go build -o go-demo.exe .
prism record --duration 10s -- .\go-demo.exe -files=false
```

Prism prints a summary of the captured session: one profile per kind (CPU, wall-clock, heap, mutex, block, goroutines), each with a short id.

> **Why it works:** the demo already imports the Prism agent. In your own program add one line, see [Profile your own Go program](#profile-your-own-go-program).

## 3. Open the UI

```powershell
prism ui --open
```

Prism prints `prism UI: http://127.0.0.1:7878/` and opens your browser. Leave the terminal running; press **Ctrl+C** to stop.

In the UI:

1. Click the **session** on the home page, then a **cpu** profile.
2. The **flame graph** shows where time goes. Wide boxes are expensive. Click a box to zoom, hover for numbers.
3. Open **Findings** for a plain-language list of likely hot spots.
4. Switch to **Top** for the hottest functions as a table (works from the keyboard).
5. Try **Time** (heat map) to see *when* the time was spent, and drag across a stretch to zoom into it.

## 4. Record a .NET program

```powershell
cd ..\dotnet-demo
dotnet build -c Release
prism record --duration 10s -- dotnet bin\Release\net8.0\dotnet-demo.dll all 15
prism ui
```

You get CPU, wall-clock, allocations, lock contention and threads in one session. No `dotnet-trace`, no code change: the .NET helper is bundled with Prism.

To attach to a program that is already running:

```powershell
prism targets                    # lists .NET processes and Go programs Prism can see
prism record --pid 12345 --duration 15s
```

## 5. Did my fix help?

Record again after a change, then compare:

```powershell
prism sessions                   # find the two session or profile ids
prism diff <before> <after>      # CLI view
```

or use the **Diff** view in the UI. Red is worse, green is better. To fail CI when something regresses, see [CI.md](CI.md).

## Profile your own Go program

Pick whichever needs the least change:

| Option | Code change | Command |
|---|---|---|
| **A. `go test`** | none | `prism record --go-test . --kinds cpu,memory` |
| **B. Existing `net/http/pprof`** | none | `prism record --url http://localhost:6060 --duration 15s` |
| **C. The Prism agent** | one import | see below |

Option C, in your program's `main.go`:

```go
import _ "github.com/rajkumarGosavi/prism/agents/go"
```

```powershell
go get github.com/rajkumarGosavi/prism/agents/go
go build -o myapp.exe .
prism record --duration 15s -- .\myapp.exe
```

The agent only starts when Prism launches the program (it sets `PRISM_AGENT=1`), and only listens on `127.0.0.1`. To attach to an already-running program, start the agent yourself with `agent.Start(agent.Options{})` (see the [user guide](USER_GUIDE.md#4a-recording)).

## Profile your own .NET program

It must run on **.NET 8 or newer**. No change is needed:

```powershell
prism record --duration 15s -- dotnet .\bin\Release\net8.0\MyApp.dll
# or attach:
prism record --pid <pid> --duration 15s
```

## Import a profile you already have

```powershell
prism import cpu.pb.gz                       # Go pprof
prism import trace.speedscope.json --kind wall
prism import stacks.folded --kind cpu         # collapsed stacks
```

PowerShell does not expand `*.pb.gz` for native programs; pass the files as a list:
`prism import (Get-ChildItem *.pb.gz)`.

## Where to go next

- [User guide](USER_GUIDE.md): every command and view, and recipes ("find a memory leak", "find lock contention").
- [Troubleshooting](TROUBLESHOOTING.md) if something did not work.
- [Known limitations](LIMITATIONS.md).
