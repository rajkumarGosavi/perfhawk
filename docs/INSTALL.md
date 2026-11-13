# Install, upgrade and uninstall (Windows)

**Supported:** Windows 10 and 11, x64. (ARM64 runs under emulation, untested. Linux and macOS are planned.)
**Needs:** nothing to install Perfhawk. Go and .NET are only needed for the things you profile.

## Option 1: installer script (recommended)

```powershell
irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1 | iex
```

What it does, and nothing else:

1. Finds the latest release on GitHub (or the one you ask for).
2. Downloads `perfhawk-<version>-windows-x64.zip` and `SHA256SUMS`.
3. Refuses to continue if the checksum does not match.
4. Unpacks to `%LOCALAPPDATA%\Programs\perfhawk`.
5. Adds that folder to your **user** `PATH`. No administrator rights, no registry changes besides that.

You can read it first: <https://github.com/rajkumarGosavi/perfhawk/blob/main/scripts/install.ps1>.

Pin a version or choose a folder:

```powershell
& ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1))) -Version v0.1.0 -InstallDir C:\Tools\perfhawk
```

Open a **new** terminal afterwards so `PATH` is refreshed.

## Option 2: manual zip

1. Download `perfhawk-<version>-windows-x64.zip` and `SHA256SUMS` from the [Releases page](https://github.com/rajkumarGosavi/perfhawk/releases).
2. Verify the hash:
   ```powershell
   (Get-FileHash .\perfhawk-<version>-windows-x64.zip -Algorithm SHA256).Hash.ToLower()
   ```
   It must equal the line for that file in `SHA256SUMS`.
3. Unzip anywhere. Keep **`perfhawk-dotnet.exe` in the same folder as `perfhawk.exe`**.
4. Optional: add that folder to `PATH`.

The zip contains `perfhawk.exe`, `perfhawk-dotnet.exe`, `README.txt`, `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `CHANGELOG.md` and `examples\`.

## Windows security warnings

If Windows SmartScreen says "Windows protected your PC", see [Troubleshooting](TROUBLESHOOTING.md#windows-protected-your-pc-or-the-file-was-deleted-by-antivirus). Perfhawk needs to read other processes to profile them, which some antivirus products flag.

## Check it works

```powershell
perfhawk --version
perfhawk --help
```

## Upgrade

Run the installer again. It replaces the files in place. If Perfhawk is running (for example `perfhawk ui`), close it first; the installer will tell you. Your profiles are untouched, and the store upgrades itself on first use.

## Uninstall

```powershell
& ([scriptblock]::Create((irm https://github.com/rajkumarGosavi/perfhawk/releases/latest/download/install.ps1))) -Uninstall
```

This removes the program folder and the `PATH` entry. It **keeps your profiles**. To remove those too, delete the data folder:

- `%LOCALAPPDATA%\perfhawk`, and `%APPDATA%\perfhawk` if it exists (earlier versions stored data there)
- or the folder you chose with `--home` / `PERFHAWK_HOME`

Manual installs: delete the folder you unzipped to and, if you added it, its `PATH` entry.

## Build from source

See [CONTRIBUTING.md](../CONTRIBUTING.md#building-from-source).
