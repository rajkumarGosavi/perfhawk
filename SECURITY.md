# Security policy

## Reporting a vulnerability

Please **do not open a public issue** for a security problem. Report it privately using GitHub's
["Report a vulnerability"](https://github.com/rajkumarGosavi/prism/security/advisories/new) form on this repository.

Include what you found, the steps to reproduce it, the Prism version (`prism --version`) and your Windows version.
You can expect an acknowledgement within 3 working days and a fix or mitigation plan within 14 days for confirmed issues.
We will credit you in the release notes unless you prefer otherwise.

## Supported versions

Only the latest release receives security fixes while Prism is below 1.0.

## Trust model

Prism is a **local, single-user** tool. These properties are intended and tested:

- The UI server binds to `127.0.0.1` only and rejects requests whose `Host` or `Origin` is not loopback (DNS-rebinding and cross-site request protection).
- Responses carry `X-Frame-Options: DENY`, a restrictive `Content-Security-Policy` and `X-Content-Type-Options: nosniff`.
- The Go agent listens on loopback only and applies the same `Host` check.
- Prism sends no telemetry and makes no network requests of its own, other than to a `--url` you give it.

Known limits, which are acceptable under that model but mean **you should not expose Prism on a network**:

- There is no authentication. Anyone who can reach the port can read your profiles.
- Decompression of imported files and uploads are not size-limited beyond a 2 GiB request cap, so a hostile file can use a lot of memory.
- Profiles can contain function names, file paths and, for memory or wall-clock captures, data that reveals how your program works. Treat them as sensitive before sharing.

## Verifying downloads

Each release publishes `SHA256SUMS`. The installer checks it for you; to check by hand:

```powershell
(Get-FileHash .\prism-<version>-windows-x64.zip -Algorithm SHA256).Hash.ToLower()
```

and compare with the matching line in `SHA256SUMS`.
