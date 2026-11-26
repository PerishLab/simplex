# Simplex

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request, acceptance, version and release policy;
this file keeps repository-specific charter and constraints without copying it.

## Standing

`simplex` is a **candidate bailiwick** transitioning from its completed
9-domain cold-start specification into `linux-x86_64` native non-LLVM compiler
implementation (`Project #13 · Simplex · Native`): the explored question,
non-goals, native records, exit condition, governed verification harness and
6-crate cross-layer ABI spine exist, while complete machine-code emission and a
released Depot `SKILL.md` promise remain `unknown` until `N0–N4` land.

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

Simplex owns the `.sx` general-purpose language specification, golden
`pass / reject` specimen suites (`cases/*`), grammar, type and borrow checker,
compile-time evaluation and typed decorators (`comptime` and `meta`), non-LLVM
native code generator, ELF64/`.so` linker and stackful `Fiber` runtime.

## Workspace topology

The Rust workspace forms a strict zero-cycle DAG across six crates:

- `crates/lib` (`simplex`): Zero-backtracking lexer, parser, canonical
  formatter, local semantic/borrow/`comptime`/`dylib`/`meta` checker and 9-domain
  golden verifier (`38 pass, 140 reject`).
- `crates/hir` (`simplex-hir`, depends on `simplex`): Monomorphized Typed HIR,
  `sx$` deterministic symbol mangling and System V AMD64 byte layout/passing
  contracts (`Footprint`, `Passing`).
- `crates/elf` (`simplex-elf`, zero dependencies): Dual-mode `linux-x86_64`
  ELF64 image planner (`Standalone` static `ET_EXEC`, `Dynamic` `PT_INTERP` PIE,
  and `Shared` `.so` library) and relocation contracts.
- `crates/runtime` (`simplex-runtime`, zero dependencies): `linux-x86_64`
  syscall numbers, `64 KiB + 4 KiB PROT_NONE` `Fiber` stack constants,
  callee-saved `Context` frame (`rbx, rbp, r12..r15, rsp, rip`) and `sx$rt$*`
  runtime hook symbols.
- `crates/emit` (`simplex-emit`, depends on `simplex-hir`, `simplex-elf`,
  `simplex-runtime`): Linear IR, System V register file (`rdi..r9` args,
  `rax+rdx` pair returns, `rbx, r12..r15` callee-saved) and `x86_64` codegen
  entry point.
- `crates/cli` (`simplex-cli`): `simplex {check, format, build}` binary and
  black-box process execution tests (`crates/cli/tests/`).

## Core objects

- **Token**: One lexically classified `.sx` source span produced without
  backtracking or comment channels.
- **Code**: A deterministic law identifier naming the exact syntactic or
  semantic rule that refuses an invalid `.sx` specimen.
- **Suite**: One orthogonal domain directory (`cases/<domain>`) pairing pure
  `pass/*.sx` and `reject/*.sx` specimens with a `suite.toml` refusal manifest.
- **Unit**: One monomorphized Typed HIR compilation module carrying exact SysV
  layouts, `.rodata` assets, vtables and routines.
- **Plan**: One `linux-x86_64` ELF64 emission specification (`Standalone`,
  `Dynamic` or `Shared`) pairing `.text`/`.rodata` bytes with symbols and
  relocations.

## Invariants

- **Truth over intention**: Keep implemented truth in `AGENTS.md` and target
  architecture in [DESIGN.md](DESIGN.md) and native Issues.
- **Pure `.sx` specimens**: `.sx` files in `cases/*` never carry inline
  expectation comments; every `reject/*.sx` file on disk must be explicitly
  listed with its exact `Code` in that domain's `suite.toml`, and unlisted or
  missing files fail closed.
- **Frozen `linux-x86_64` ABI contract**: All backend crates obey System V AMD64
  register/pair/`sret` passing, `sx$<ns>$<module>$<name>` /
  `sx$vt$<ns>$<module>$<Type>$<Interface>` symbol names, dual-mode ELF64 output
  (zero-interpreter standalone when dependency-free; `PT_INTERP` when linking
  `dylib` or `extern "c"`), and `64 KiB + 4 KiB PROT_NONE` `Fiber` stacks.
- **Orthogonal systems-language scope**: Simplex benchmarks directly against
  C++, Rust and Zig as a systems-grade general-purpose programming language; it
  does not solve markup, layout, styling or other specialized domain-specific
  language (DSL) problems and refuses kitchen-sink scope expansion.
- **Zero silent weakening**: All Rust source obeys domain `singleword`,
  comment-ban and structural limits without exemptions.

## Non-goals

- Solving or absorbing markup, layout, styling or other specialized/future
  domain-specific languages (DSLs).
- No LLVM or Cranelift backend dependency.
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
