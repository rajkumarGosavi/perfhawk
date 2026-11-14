# ReadyToRun Method Names Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Name frames that land in precompiled (ReadyToRun) .NET code, such as `System.Private.CoreLib.dll+0x3c485b`, as `Namespace.Type::Method` in ETW-based profiles.

**Architecture:** At symbolization time, for each image loaded by the target, read the ReadyToRun tables straight from the DLL on disk: locate the READYTORUN header, enumerate the method entry points, map each to the code ranges in the RuntimeFunctions table, and take names from the DLL's embedded ECMA-335 metadata. The result is a sorted list of (start RVA, size, name) added to the existing image symbols in `Symbolizer`. No change to the ETW sessions.

**Tech Stack:** Rust, the existing `object` crate (PE parsing), hand-written readers for the NativeFormat arrays and ECMA-335 metadata tables. No new dependencies.

**Spec:** none separate. Requirements come from `docs/FOLLOWUPS.md` ("From Phase 2 M3": .NET managed frames, PDB names) and the live results in the PR #4 thread: rundown reports only JIT-compiled methods, so CoreLib frames stay `module+offset`. Format reference: `docs/design/coreclr/botr/readytorun-format.md` in dotnet/runtime; native-format reader reference: `src/coreclr/tools/Common/Internal/NativeFormat/NativeFormatReader.cs` in dotnet/runtime.

## Global Constraints

- Windows x64 images only (12-byte `RUNTIME_FUNCTION` entries). Other architectures, composite images and single-file bundles return no symbols; they must never panic or error out the capture.
- No new crate dependencies (`cargo deny` licence list stays unchanged).
- Malformed, truncated or non-ReadyToRun input returns `None`/empty, never panics, and never reads out of bounds.
- Names use the same form as JIT events: `Namespace.Type::Method` via `symbolize::method_name`; nested types as `Outer+Inner`; empty namespace gives just the type.
- Parsing one image must stay under 200 ms for `System.Private.CoreLib.dll` in a release build.
- Facts taken from the spec and not yet checked against a real file are marked **VERIFY** and must be checked in the task that uses them.

## Review Focus

- A method whose code has several `RUNTIME_FUNCTION`s (funclets for exception handlers): every part resolves to the same method name.
- Hot/cold split methods: cold parts are not named (no `HotColdMap` support); they must stay `module+offset`, not get a wrong name.
- Nested types and generic types (`List`1`): the name is `Outer+Inner` and keeps the backtick arity, matching the JIT-event naming.
- A DLL that changed on disk after the target loaded it (different build): RVAs would map to wrong names. Compare the image's PE timestamp/size with the `ImageLoad` size and skip on mismatch.
- A managed DLL with IL only (no ReadyToRun header), a native DLL, and a zero-byte or truncated file: all return empty without error.

---

## File Structure

- Create `crates/perfhawk-capture/src/symbolize/r2r/mod.rs`: `pub fn r2r_symbols(path: &Path) -> Option<Vec<R2rSym>>`, the glue and the public types.
- Create `crates/perfhawk-capture/src/symbolize/r2r/header.rs`: PE data-directory walk to the READYTORUN header and its section list.
- Create `crates/perfhawk-capture/src/symbolize/r2r/native.rs`: NativeFormat unsigned decoding and the sparse `NativeArray` reader.
- Create `crates/perfhawk-capture/src/symbolize/r2r/metadata.rs`: ECMA-335 reader for MethodDef names with declaring types.
- Modify `crates/perfhawk-capture/src/symbolize/mod.rs`: `Symbolizer::add_image` also loads R2R symbols.
- Create `crates/perfhawk-capture/tests/r2r.rs`: tests that use an installed .NET runtime (skip when none) and a small checked-in fixture.
- Create `crates/perfhawk-capture/tests/fixtures/MiniLib.dll`: a few-KB IL-only assembly built from a tiny C# source, committed with its source in `tests/fixtures/MiniLib.cs`.

---

### Task 1: Locate the ReadyToRun header and sections

**Files:**
- Create: `crates/perfhawk-capture/src/symbolize/r2r/mod.rs`, `crates/perfhawk-capture/src/symbolize/r2r/header.rs`
- Modify: `crates/perfhawk-capture/src/symbolize/mod.rs` (add `mod r2r;`)
- Test: `crates/perfhawk-capture/tests/r2r.rs`

**Interfaces:**
- Produces: `pub(super) struct R2rImage<'a> { pub data: &'a [u8], pub runtime_functions: Range<usize>, pub method_def_entry_points: Range<usize>, pub metadata: Range<usize> }` as byte ranges into `data` (`metadata` is the CLI metadata directory from the COR header), plus `pub(super) fn open(data: &[u8]) -> Option<R2rImage<'_>>` and `fn rva_range(pe: &PeFile64, rva: u32, size: u32) -> Option<Range<usize>>` (RVA to file range through the section table).

- [ ] **Step 1: Write failing tests in `tests/r2r.rs`:** `rejects_non_pe_and_truncated_input` (empty, 64 random bytes, first 200 bytes of a real DLL all give `None`), and `finds_r2r_sections_in_corelib` (uses `find_corelib()`: `DOTNET_ROOT` or `C:\Program Files\dotnet\shared\Microsoft.NETCore.App\<highest 8.*>\System.Private.CoreLib.dll`; skip with a message when missing). Assert signature `0x00525452`, `runtime_functions` and `method_def_entry_points` are non-empty, and the runtime-function range length is a multiple of 12.
- [ ] **Step 2: Run** `cargo test -p perfhawk-capture --test r2r` and confirm they fail to compile or fail.
- [ ] **Step 3: Implement `open`.** Use the `object` crate's PE reader to get the COM descriptor data directory (index 14), read `IMAGE_COR20_HEADER`, take `ManagedNativeHeader` (the data directory at offset 64 of that header), then parse `READYTORUN_HEADER` (signature `0x00525452`, u16 major, u16 minor, u32 flags, u32 section count, then 12-byte sections: u32 type, u32 RVA, u32 size). Section types: RuntimeFunctions 102, MethodDefEntryPoints 103. Require major version >= 2 and x64 machine type. **VERIFY** the COR header offset 64 and section layout on the installed CoreLib; print the major/minor version in the test output.
- [ ] **Step 4: Run the tests and confirm pass.**
- [ ] **Step 5: Commit** `feat(symbolize): locate ReadyToRun header and sections`.

### Task 2: NativeFormat decoding and the sparse array

**Files:**
- Create: `crates/perfhawk-capture/src/symbolize/r2r/native.rs`
- Test: unit tests inside `native.rs`

**Interfaces:**
- Produces: `pub(super) fn decode_unsigned(data: &[u8], offset: usize) -> Option<(u32, usize)>` (value, next offset) and `pub(super) struct NativeArray<'a>` with `fn new(data: &'a [u8], offset: usize) -> Option<Self>`, `fn len(&self) -> u32`, `fn get(&self, index: u32) -> Option<usize>` (offset of element `index`, `None` when empty).

- [ ] **Step 1: Write failing tests:** `decode_unsigned_all_widths` with the 1- to 5-byte encodings (the low bits of the first byte give the length: `xxx0` one byte, `xx01` two, `x011` three, `0111` four, `1111` five followed by a raw u32), and `array_lookup_matches_reference` that builds a small sparse array in the test with a helper encoder following the BOTR spec (header with element count and offset size, per-16 block index, binary-tree blocks) and checks `get` for present and absent indexes, including indexes in different blocks and a count that is not a multiple of 16.
- [ ] **Step 2: Run and confirm failure.**
- [ ] **Step 3: Implement** both pieces following `NativeFormatReader.cs` (`NativeArray.TryGetAt`). Every offset read is bounds-checked and returns `None` on overrun.
- [ ] **Step 4: Run and confirm pass.**
- [ ] **Step 5: Commit** `feat(symbolize): NativeFormat reader for ReadyToRun method tables`.

### Task 3: MethodDef names from ECMA-335 metadata

**Files:**
- Create: `crates/perfhawk-capture/src/symbolize/r2r/metadata.rs`, `crates/perfhawk-capture/tests/fixtures/MiniLib.cs`, `crates/perfhawk-capture/tests/fixtures/MiniLib.dll`
- Test: `crates/perfhawk-capture/tests/r2r.rs`

**Interfaces:**
- Produces: `pub(super) fn method_names(metadata: &[u8]) -> Option<Vec<String>>`, where element `i` is the `Namespace.Type::Method` name of MethodDef row `i + 1`. Uses `symbolize::method_name` for formatting.

- [ ] **Step 1: Build the fixture.** `MiniLib.cs` defines namespace `Mini` with a class `Outer` (methods `Run`, `Stop`), nested class `Outer.Inner` (method `Deep`), generic class `Box<T>` (method `Get`), and a top-level type with an empty namespace (method `Global`). Build with `dotnet build` and commit only the resulting `MiniLib.dll` and the source.
- [ ] **Step 2: Write the failing test** `metadata_names_for_minilib`: assert the list contains `Mini.Outer::Run`, `Mini.Outer::Stop`, `Mini.Outer+Inner::Deep`, ``Mini.Box`1::Get``, and `Global`-type naming without a leading `::` or dot. Add `metadata_rejects_garbage` (empty and random bytes give `None`).
- [ ] **Step 3: Implement.** Parse the `BSJB` root, find streams `#~` (or `#-`) and `#Strings`, compute every table's row size from the heap-size flags and row counts (all 45 table kinds are needed to locate `MethodDef` (0x06), `TypeDef` (0x02) and `NestedClass` (0x29)), then for each TypeDef assign its method range from `MethodList` to the next TypeDef's. Nested types resolve through `NestedClass`, building `Outer+Inner`.
- [ ] **Step 4: Run and confirm pass; also run against CoreLib's metadata** in a second test that asserts `System.String::get_Length` is present and the list has more than 20,000 entries.
- [ ] **Step 5: Commit** `feat(symbolize): MethodDef names from ECMA-335 metadata`.

### Task 4: Method ranges and `r2r_symbols`

**Files:**
- Modify: `crates/perfhawk-capture/src/symbolize/r2r/mod.rs`
- Test: `crates/perfhawk-capture/tests/r2r.rs`

**Interfaces:**
- Consumes: `open`, `NativeArray`, `decode_unsigned`, `method_names` from tasks 1 to 3.
- Produces: `pub struct R2rSym { pub start: u64, pub size: u64, pub name: String }` (RVA-relative, sorted by `start`, non-overlapping) and `pub fn r2r_symbols(path: &Path) -> Option<Vec<R2rSym>>`.

- [ ] **Step 1: Write failing tests** against CoreLib: `corelib_symbols_are_sorted_and_named` (more than 10,000 symbols, sorted by `start`, `size > 0`, a known method such as `System.String::get_Length` present or, if inlined away, `System.String::Concat`; pick one that exists and note which), and `corelib_parse_is_fast` (release-profile only: under 200 ms; in debug just print the time). Add `non_r2r_dll_returns_none` using `MiniLib.dll`.
- [ ] **Step 2: Run and confirm failure.**
- [ ] **Step 3: Implement `r2r_symbols`.** For each MethodDef row, `NativeArray::get(row)` gives the element; decode it with `decode_unsigned`; the runtime-function index is `v >> 1` when the low bit is 0, otherwise `v >> 2` (inline or backward fixup list follows; ignore the fixups). A method owns runtime functions `[idx, next_idx)` where `next_idx` is the next larger index among all methods (or the table end); give every one of those ranges the method's name (funclets). Each runtime function is `(BeginAddress u32, EndAddress u32, UnwindData u32)`: size is `End - Begin`. **VERIFY** this layout (the spec table describes the three fields differently) by checking `Begin < End` and monotonic order on CoreLib, and fix this task's code and the Global Constraints if it differs. Skip entries whose index is out of range. Sort by `start`.
- [ ] **Step 4: Run and confirm pass.**
- [ ] **Step 5: Commit** `feat(symbolize): r2r_symbols reads ReadyToRun method ranges`.

### Task 5: Use R2R symbols in the `Symbolizer`

**Files:**
- Modify: `crates/perfhawk-capture/src/symbolize/mod.rs` (`Symbolizer::add_image`)
- Test: `crates/perfhawk-capture/tests/symbolize.rs`

**Interfaces:**
- Consumes: `r2r_symbols(path) -> Option<Vec<R2rSym>>`.
- Produces: no new public API; `Symbolizer::resolve` returns `Frame::Named` for addresses inside R2R methods.

- [ ] **Step 1: Write failing test** `resolves_address_inside_corelib_r2r_method`: take a symbol from `r2r_symbols` for CoreLib, `add_image(base, size, path)` with `base = 0x7ff8_0000_0000`, resolve `base + sym.start + 1` and assert `Frame::Named(sym.name)`; resolve `base + 0` and assert it stays `ModuleOffset`. Skip when no runtime is installed.
- [ ] **Step 2: Run and confirm failure.**
- [ ] **Step 3: In `add_image`**, when `go_symbols(path)` is empty, load `r2r_symbols(path)` and convert into the existing per-image symbol list (same `GoSym` shape: start RVA, size, name). Skip the image when its on-disk size differs from the `size` the ETW image-load reported by more than the PE header padding (guards the changed-DLL case; log nothing, resolve as `module+offset`).
- [ ] **Step 4: Run** `cargo test -p perfhawk-capture` and `cargo clippy -p perfhawk-capture --all-targets`, confirm both clean.
- [ ] **Step 5: Commit** `feat(symbolize): name frames in ReadyToRun images`.

### Task 6: Live verification and docs

**Files:**
- Modify: `docs/FOLLOWUPS.md`, `docs/LIMITATIONS.md`

- [ ] **Step 1:** Rebuild release (`cargo build --release -p perfhawk-cli`). From an elevated shell run `./scripts/samples/record.ps1 -App jellyfin -Seconds 10 -Kinds "block_io,off_cpu"`.
- [ ] **Step 2:** `perfhawk show <off_cpu id> --sort total --top 40`. Expected: `System.Private.CoreLib.dll+0x...` frames replaced mostly by `System.Threading...::Method` names. Record the before and after counts of unresolved frames from the session warning.
- [ ] **Step 3:** Update the .NET entry in `docs/FOLLOWUPS.md` (ReadyToRun now named; remaining gaps: `coreclr.dll`, `ntdll.dll`, cold parts, composite and ARM64 images) and add the supported-platform note to `docs/LIMITATIONS.md`.
- [ ] **Step 4: Commit and push** `docs: ReadyToRun naming results and remaining gaps`.
