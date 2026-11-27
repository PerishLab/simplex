# Simplex

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request, acceptance, version and release policy;
this file keeps repository-specific charter and constraints without copying it.

## Standing

`simplex` is a **candidate bailiwick** advancing through `Cycle 3` (`Package`,
`Project #13 · Simplex · Native`): `crates/package` (4-tuple `Coordinate`,
lockfile-free manifests, `simplex.seal/v1` `x.y.z` audit, Create-Only S3 SigV4
store) and `crates/hir::build` (cross-package monomorphized `import` and 4-tuple
`sx_<scope>_<name>_<x>_<y>_<z>_<channel>` `dylib` compilation plans) are live;
CLI `seal`/`publish`/package-mode `build` wiring and end-to-end S3 black-box
trials land in `P4`, and a released Depot `SKILL.md` promise remains `unknown`
until formal release.

## Explored question

How a general-purpose programming language and native compiler built directly
around structural clarity, ownership memory safety, fast non-LLVM machine-code
generation and first-class dynamic libraries (`dylib`) can make every program
that passes compilation inherently clean to read and sub-second to verify under
high-density human-agent collaboration. Previously, PerishLab carried that cost
externally through host-language syntax projections (`Ectropy`), heavy LLVM
rebuild serialization (`Plumb Guard` leases and cache parcels), and manual
discipline against host-language escape hatches.

## Responsibility

Simplex owns the `.sx` general-purpose language specification, golden `pass / reject` specimen suites (`cases/*`), grammar, type and borrow checker, `comptime`/`meta` evaluator, 4-tuple library manifest/seal/audit and Create-Only S3 SigV4 store (`simplex-package`), cross-package monomorphized `HIR -> LIR` and zero-stack `Fiber<T>` lowering (`simplex-hir`, `simplex-ir`), non-LLVM `x86_64`/`aarch64` native code generator (`simplex-target`), and zero-linker `ELF64`/`PE32+`/`Mach-O 64` binary emitter (`simplex-object`).

## Workspace topology

The Rust workspace forms a strict zero-cycle DAG across seven crates (`ir`, `object`, and `target` have zero dependency on `.sx` syntax):

- `crates/lib` (`simplex`): Zero-backtracking lexer, parser, canonical
  formatter, local semantic/borrow/`comptime`/`dylib`/`meta` checker and 9-domain
  golden verifier (`38 pass, 140 reject`).
- `crates/package` (`simplex-package`, depends on `simplex`): 4-tuple
  `Coordinate` / `Version` (`model`), `simplex.package/v1` &
  `simplex.workspace/v1` (`manifest`), deterministic UStar `source.tar`
  (`archive`), canonical `pub` API/ABI `Contract` (`surface`), `simplex.seal/v1`
  (`seal`), 3-tier `x.y.z` auditor (`audit`), pure-Rust `AWS SigV4` (`signing`),
  and Create-Only `If-None-Match: "*"` S3 + SHA-256 local cache (`store`).
- `crates/ir` (`simplex-ir`, zero dependencies): Target-neutral Block-Arg SSA
  `Module`, $O(N)$ `verify`, `simplify`, and zero-stack `Fiber<T>` continuation
  frame lowerer (`prepare`).
- `crates/hir` (`simplex-hir`, depends on `simplex`, `simplex-ir`,
  `simplex-package`): Standalone and cross-package (`Linked`, `Graph`, `Plan`,
  `build`) monomorphized Typed HIR lowerer, 4-tuple namespace/symbol isolation,
  `sx_<scope>_<name>_<x>_<y>_<z>_<channel>` dynamic library planner, and
  target-neutral `LIR` bridge (`translate`, `bridge`).
- `crates/object` (`simplex-object`, zero dependencies): Pure-Rust unified
  `ELF64` / `PE32+` / SHA-256 signed `Mach-O 64` `Artifact` emitter (`emit`).
- `crates/target` (`simplex-target`, depends on `simplex-ir`, `simplex-object`):
  Loop-aware linear-scan register allocator (`machine`, `allocate`), `x86_64`/SSE2
  (`intel`, `linux` SysV AMD64, `windows` Microsoft x64) and `aarch64` (`arm`,
  `apple` AAPCS64) native machine-code backends.
- `crates/cli` (`simplex-cli`): `simplex {check, format, build [--target] [--dylib] [-o]}`
  with `unlink`-before-write atomic output replacement and tri-OS offline +
  `SIMPLEX_REMOTE` real-hardware black-box process trials (`crates/cli/tests/`).

## Core objects

- **Token**: One lexically classified `.sx` source span produced without
  backtracking or comment channels.
- **Code**: A deterministic law identifier naming the exact syntactic or
  semantic rule that refuses an invalid `.sx` specimen.
- **Suite**: One orthogonal domain directory (`cases/<domain>`) pairing pure
  `pass/*.sx` and `reject/*.sx` specimens with a `suite.toml` refusal manifest.
- **Coordinate**: One mandatory 4-tuple library identity `(scope, name, x.y.z, channel)`
  addressing immutable `<scope>/<name>/<x.y.z>/<channel>/{source.tar, seal.toml}` objects
  and collision-free `{scope}${name}${major}_{minor}_{patch}${channel}` HIR namespaces.
- **Module**: One target-neutral `simplex-ir` Block-Arg SSA compilation unit
  lowered to pure synchronous routines after `prepare`.
- **Artifact**: One unified `simplex-object` code, data, symbol, import and
  `Fixup` relocation bundle targeting `Elf`, `Portable`, or `Mach`.

## Invariants

- **Truth over intention**: Keep implemented truth in `AGENTS.md` and target
  architecture in [DESIGN.md](DESIGN.md) and native Issues.
- **Pure `.sx` specimens**: `.sx` files in `cases/*` never carry inline
  expectation comments; every `reject/*.sx` file on disk must be explicitly
  listed with its exact `Code` in that domain's `suite.toml`, and unlisted or
  missing files fail closed.
- **Lockfile-free Create-Only S3 & mechanical `x.y.z` law**: `simplex.toml`
  admits only complete 4-tuple coordinates or `{ workspace = "*" }`, forbids
  `simplex.lock`, gates `x.y.z` transitions via `simplex.seal/v1`, enforces
  zero-fallback SHA-256 verification and `If-None-Match: "*"` immutability on S3,
  and isolates multi-version/multi-channel packages across static `import` and
  `dylib` boundaries without collision.
- **Frozen tri-OS binary & zero-stack `Fiber<T>` contract**: `simplex build` emits zero-SDK `linux-x86_64` (`ELF64` `.so`), `windows-x86_64` (`PE32+` `.exe`/`.dll`), and `darwin-aarch64` (`16 KiB`-aligned signed `Mach-O 64` `lib*.dylib` with `LC_UUID` and `LC_DYLD_CHAINED_FIXUPS`), passing byte-identical `N1–N4` and `dylib` hot-swap black-box trials across Linux, Windows 11, and Apple Silicon.
- **Orthogonal systems-language scope**: Simplex benchmarks directly against
  C++, Rust and Zig as a systems-grade general-purpose programming language; it
  does not solve markup, layout, styling or other specialized domain-specific
  language (DSL) problems and refuses kitchen-sink scope expansion.
- **Zero silent weakening**: All Rust source obeys domain `singleword`,
  comment-ban and structural limits without exemptions.

## Non-goals

- Solving or absorbing markup, layout, styling or other specialized/future
  domain-specific languages (DSLs).
- No LLVM, Cranelift or external system linker/SDK backend dependency.
- No tracing garbage collector.
- No class or struct inheritance hierarchy.
- No built-in white-box unit-test syntax (`test`, `@test`) or conditional
  compilation split (`@cfg`) in `.sx` source.
- No source comments, `if` statements (`only-switch`), implicit type coercions
  or multi-word local identifiers in `.sx` source.
- No `simplex.lock` lockfile, fuzzy version ranges, prebuilt remote `dylib`
  artifacts or implicit cross-version/cross-channel fallback.
- No replacement for domain delivery, norm, black-box trial or distribution
  authorities (`Concord`, `Plumb`, `Ectropy`, `Seatrial`, `Wharf`).

## Native records

- Active native-compiler planning lives in [Project #13 (`Simplex · Native`)](https://github.com/orgs/PerishLab/projects/13);
  closed cold-start history is archived in [Project #12 (`Simplex · Cold-start`)](https://github.com/orgs/PerishLab/projects/12).
- Decisions, specimen reviews, boundary changes, progress and acceptance live
  only in `PerishLab/simplex` GitHub Issues and pull requests.
- The repository carries no workflow, no skill source and no release notes.

## Verification

Every repository change must pass `plumb doctor .`, `ectropy .`,
`cargo fmt --all --check`,
`cargo clippy --locked --workspace --all-targets -- -D warnings`,
`cargo test --locked --workspace`, `cargo run --locked -p simplex-cli -- check`
and `plumb guard`.

## Exit condition

- **Retirement**: Retire if host languages combined with `Ectropy` and `Plumb`
  satisfy PerishLab's structural clarity, capability isolation and build/link
  latency needs at lower long-term cost than maintaining a dedicated compiler.
- **Split**: If Simplex's non-LLVM backend or `dylib` ABI substrate becomes
  independently consumed outside `.sx` compilation, extract that substrate into
  its own bailiwick.
