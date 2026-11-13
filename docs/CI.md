# Regression checks in CI

`perfhawk check` compares a profile with a baseline and exits non-zero when a function got meaningfully more expensive. It reads only the local profile store (or files you import first), so it needs no server.

```
perfhawk check <HEAD> --baseline <BASE> [--sample-type NAME] [--threshold PCT] [--min-value POINTS] [--limit N] [--json]
```

`HEAD` and `BASE` are profile references: an id, a unique id prefix or a name, as for `perfhawk diff`. Import files first with `perfhawk import`.

## What counts as a regression

A function regresses when **both** hold:

- its self value grew by more than `--threshold` percent (default 10; scaled by each profile's recorded duration when both have one, so a shorter or longer run does not hide or invent a regression), and
- its share of its own profile grew by at least `--min-value` percentage points (default 1.0).

A function that is **new** in the head regresses when it holds at least `--min-value` points of the profile. Improvements never fail the check. Shares come from each profile's own total. A function is identified by its exact name, so a rename shows up as one regression (the new name) and one improvement.

## Exit codes

| code | meaning |
|---|---|
| 0 | no regression |
| 1 | at least one regression |
| 2 | usage or data error: unknown profile, no common sample type, empty baseline, bad option |

`--json` prints `{ ok, sampleType, baseTotal, headTotal, regressions: [{ name, baseSelf, headSelf, growthPct, deltaPoints }] }`. `growthPct` is `null` for a function that is new in the head.

## GitHub Actions example

This job profiles a benchmark on the pull request, fetches the baseline profile that the main branch saved earlier, and fails the job when something regressed. Adapt the benchmark command to your project.

```yaml
name: perf
on: pull_request

jobs:
  perf-check:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: actions/setup-go@v5
        with: { go-version: stable }
      - name: Install perfhawk
        run: cargo install --path crates/perfhawk-cli --locked   # or download a release build

      # 1. Profile the benchmark on this commit.
      - name: Profile (head)
        run: go test -run '^$' -bench . -cpuprofile head.pb.gz ./...

      # 2. The baseline profile saved by the main branch.
      - name: Fetch baseline
        uses: actions/download-artifact@v4
        with:
          name: perf-baseline
          path: baseline
          github-token: ${{ secrets.GITHUB_TOKEN }}
          run-id: ${{ vars.BASELINE_RUN_ID }}   # or use a cache or a release asset

      # 3. Compare. Exit 1 fails the job.
      - name: Check
        run: |
          perfhawk import head.pb.gz --name head
          perfhawk import baseline/base.pb.gz --name base
          perfhawk check head --baseline base --threshold 10 --json | tee perf-check.json

      - uses: actions/upload-artifact@v4
        if: always()
        with: { name: perf-check, path: perf-check.json }
```

On the main branch, a second job uploads `head.pb.gz` as the `perf-baseline` artifact so the next pull request has something to compare with. Benchmarks are noisy: raise `--threshold`, raise `--min-value`, or compare against the median of several baseline runs if you see false alarms.

## History and series

For trends over many runs, tag recordings with `--tag series:<name>` (for example `perfhawk record --tag series:checkout --pid 1234`). The **History** view then plots one function across the sessions of that series. Without the tag, sessions are grouped by the program they profiled. See the [user guide](USER_GUIDE.md).

## Limits

- No pull request comments, dashboards or a shared server yet: those belong to a later phase.
- `perfhawk check` compares one profile with one baseline; it does not look at trends.
- Function names are matched exactly, with no fuzzy rename detection.
