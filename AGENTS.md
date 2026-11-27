# Simplex

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request, acceptance, version and release policy;
this file keeps repository-specific charter and constraints without copying it.

## Standing

`simplex` is a **candidate bailiwick** advancing from its completed 9-domain
cold-start specification and `Cycle 1` `linux-x86_64` native compiler into
`Cycle 2` (`Substrate`, `Project #13 · Simplex · Native`): the complete Cycle 2 (`Substrate`) 6-crate target-neutral compiler, zero-stack `Fiber<T>` continuation frame runtime, and zero-SDK native executable + `dylib` backends across `linux-x86_64` (`ELF64`), `windows-x86_64` (`PE32+`), and `darwin-aarch64` (`Mach-O 64` + built-in SHA-256 `CodeDirectory`) are live and hardware-verified across Linux, Windows 11, and Apple Silicon (`M4`); a
released Depot `SKILL.md` promise remains `unknown` until formal release.

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

Simplex owns the `.sx` general-purpose language specification, golden `pass / reject` specimen suites (`cases/*`), grammar, type and borrow checker, `comptime`/`meta` evaluator, target-neutral `HIR -> LIR` and zero-stack `Fiber<T>` lowering, non-LLVM `x86_64`/`aarch64` native code generator (`simplex-target`), and zero-linker `ELF64`/`PE32+`/`Mach-O 64` binary emitter (`simplex-object`).

## Workspace topology

The Rust workspace forms a strict zero-cycle DAG across six crates (`ir`, `object`, and `target` have zero dependency on `.sx` syntax):

- `crates/lib` (`simplex`): Zero-backtracking lexer, parser, canonical
  formatter, local semantic/borrow/`comptime`/`dylib`/`meta` checker and 9-domain
  golden verifier (`38 pass, 140 reject`).
- `crates/ir` (`simplex-ir`, zero dependencies): Target-neutral Block-Arg SSA
  `Module`, $O(N)$ `verify`, `simplify`, and zero-stack `Fiber<T>` continuation
  frame lowerer (`prepare`).
- `crates/hir` (`simplex-hir`, depends on `simplex`, `simplex-ir`): Multi-module
  monomorphized Typed HIR lowerer and target-neutral `LIR` bridge (`translate`,
  `bridge`).
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
