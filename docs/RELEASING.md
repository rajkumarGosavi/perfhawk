# Releasing Perfhawk

For maintainers. The engineering checklist is in [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md); the launch plan is in [LAUNCH.md](LAUNCH.md).

## What a release produces

A **draft** GitHub Release (published by a human) with:

| Asset | What |
|---|---|
| `perfhawk-<version>-windows-x64.zip` | `perfhawk.exe` (UI embedded), `perfhawk-dotnet.exe` (self-contained helper), `LICENSE`, `THIRD_PARTY_NOTICES.txt`, `CHANGELOG.md`, `README.txt`, `examples\` |
| `SHA256SUMS` | SHA-256 of the zip |
| `install.ps1` | The installer, from `scripts/install.ps1` |

Plus a Go module tag for the agent (see below).

## Steps

1. **Branch and freeze.** Work from `main`; CI must be green on Windows.
2. **Version.** Set `version` in `Cargo.toml` `[workspace.package]` and `<Version>` in `helpers/perfhawk-dotnet/PerfhawkDotnet.csproj` to the same value.
3. **Changelog.** Move `Unreleased` entries under the new version with a date in `CHANGELOG.md`.
4. **Release candidate.** Tag `v0.1.0-rc.1` (a tag containing `-` becomes a *pre-release*):
   ```powershell
   git tag -a v0.1.0-rc.1 -m "v0.1.0-rc.1"
   git push origin v0.1.0-rc.1
   ```
   The **Release** workflow builds, packages, smoke-tests and drafts the release.
5. **Test the artifact** on a clean Windows 11 machine or Windows Sandbox: run the installer against the draft (download the zip manually while it is a draft), then follow the [Quickstart](QUICKSTART.md) end to end, then uninstall.
6. **Final tag.** Repeat step 4 with `v0.1.0`. The workflow checks that the tag matches the `Cargo.toml` version.
7. **Publish the draft** on GitHub. `install.ps1` resolves "latest" to published, non-draft, non-pre-release releases only.
8. **Tag the Go agent** (it is a nested module, so its tag is prefixed with its directory):
   ```powershell
   git tag agents/go/v0.1.0
   git push origin agents/go/v0.1.0
   ```
   Verify from an empty module:
   ```powershell
   mkdir $env:TEMP\gotest; cd $env:TEMP\gotest
   go mod init example.com/t
   go get github.com/rajkumarGosavi/perfhawk/agents/go@v0.1.0
   ```
   Go's module proxy caches a version forever; **never move or delete a published tag**. If something is wrong, publish `v0.1.1`.
9. **Smoke test the published release:** `irm .../install.ps1 | iex` on a clean machine.
10. **Announce** per [LAUNCH.md](LAUNCH.md).

## Hotfix

Branch from the tag, fix, bump the patch version, tag `v0.1.1`. The workflow is the same.

## Code signing

Unsigned binaries trigger SmartScreen and antivirus warnings. Sign **`perfhawk.exe` and `perfhawk-dotnet.exe` before they are zipped**, i.e. in the Release workflow between "Third-party notices" and "Package" (there is a comment marking the spot).

Options, cheapest first:

- **SignPath Foundation**: free for open-source projects, applies via their website, integrates with GitHub Actions.
- **Azure Trusted Signing**: low monthly cost, no hardware token. Needs an Azure subscription and identity validation (can take days).
- **OV/EV certificate** from a CA: EV gives immediate SmartScreen reputation but needs a hardware token or cloud HSM.

Start identity validation early; it has lead time. After signing, add a verification step: `Get-AuthenticodeSignature .\perfhawk.exe` must be `Valid`.
Submit each release to Microsoft Security Intelligence (<https://www.microsoft.com/wdsi/filesubmission>) as a proactive false-positive check.

## Local dry run

From a Windows PowerShell prompt at the repo root (needs Rust, Node, .NET 8 SDK, cargo-about):

```powershell
cd ui; npm ci; npm run build; cd ..
cargo build --release --locked -p perfhawk-cli
dotnet publish helpers/perfhawk-dotnet -c Release -r win-x64 --self-contained `
  -p:PublishSingleFile=true -p:IncludeNativeLibrariesForSelfExtract=true `
  -p:EnableCompressionInSingleFile=true -o target/helper
./tools/release/build-notices.ps1
./tools/release/package.ps1 -Version dev-local
./tools/release/smoke-test.ps1 -Zip dist/perfhawk-dev-local-windows-x64.zip
```

## Generated files

`THIRD_PARTY_NOTICES.txt`, `dist/` and `target/` are generated and git-ignored.
