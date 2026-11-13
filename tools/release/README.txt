Perfhawk - local-first profiling workbench for Go and .NET

Quick start (PowerShell):
    .\perfhawk.exe --version
    .\perfhawk.exe doctor          # check your setup (when available)
    .\perfhawk.exe ui              # browse profiles at http://127.0.0.1:7878/

To use "perfhawk" from anywhere, add this folder to your PATH, or install with:
    irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1 | iex

Files in this folder:
    perfhawk.exe          the command-line tool and UI server
    perfhawk-dotnet.exe   helper that records .NET processes (keep next to perfhawk.exe)
    examples\          demo programs with deliberate performance problems
    THIRD_PARTY_NOTICES.txt   licences of bundled components

Documentation: https://github.com/rajkumarGosavi/perfhawk#readme
Privacy: Perfhawk runs locally, listens on 127.0.0.1 only and sends no telemetry.
