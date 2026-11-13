Prism - local-first profiling workbench for Go and .NET

Quick start (PowerShell):
    .\prism.exe --version
    .\prism.exe doctor          # check your setup (when available)
    .\prism.exe ui              # browse profiles at http://127.0.0.1:7878/

To use "prism" from anywhere, add this folder to your PATH, or install with:
    irm https://github.com/rajkumarGosavi/prism/releases/latest/download/install.ps1 | iex

Files in this folder:
    prism.exe          the command-line tool and UI server
    prism-dotnet.exe   helper that records .NET processes (keep next to prism.exe)
    examples\          demo programs with deliberate performance problems
    THIRD_PARTY_NOTICES.txt   licences of bundled components

Documentation: https://github.com/rajkumarGosavi/prism#readme
Privacy: Prism runs locally, listens on 127.0.0.1 only and sends no telemetry.
