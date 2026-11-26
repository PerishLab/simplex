# Simplex

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request, acceptance, version and release policy;
this file keeps repository-specific charter and constraints without copying it.

## Standing

`simplex` is currently a **candidate bailiwick** in its quiet design-seed
baseline: the explored question, non-goals, native records and exit condition
are established here, while the landed product condition (`SKILL.md` facade and
working compiler binary) remains `unknown` until the first executable compiler
slice ships.

## Explored question

How a programming language and native compiler built directly around
structural clarity, ownership memory safety, fast non-LLVM machine code
generation and first-class dynamic libraries (`dylib`) can make every program
that passes compilation inherently clean to read and sub-second to verify under
high-density human-agent collaboration. Previously, PerishLab carried that cost
externally through host-language syntax projections (`Ectropy`), heavy LLVM
rebuild serialization (`Plumb Guard` leases and cache parcels), and manual
discipline against host-language escape hatches.

## Responsibility

Simplex owns the `.sx` language specification, grammar, type and borrow
checker, compile-time evaluation (`comptime`), non-LLVM native code generator
and `dylib` ABI/linking contract.

## Current baseline

This repository currently holds only the candidate charter (`AGENTS.md`), initial
language design baseline ([DESIGN.md](DESIGN.md)), `LICENSE` and `.gitignore`.
No compiler crate, CLI binary, Plumb configuration or release authority exists
yet. The first real implementation Issue brings the actual workspace,
`plumb.toml` and `ectropy.toml`; do not add empty placeholder packages or
scaffolding solely to make this seed baseline look complete.

## Core objects

Core runtime and compiler objects remain `unknown` at the charter level until
the first grammar and semantic-kernel Issue lands; target language principles
are recorded in [DESIGN.md](DESIGN.md) rather than stated here as implemented
facts.

## Invariants

- **Truth over intention**: Keep implemented truth in `AGENTS.md` and unbuilt
  language design direction in [DESIGN.md](DESIGN.md) and native Issues.
- **No placeholder scaffolding**: Introduce build manifests, crates and release
  declarations only with real tested compiler functionality.
- **Zero silent weakening**: Once `plumb.toml` and `ectropy.toml` enter with the
  first code slice, all bootstrap source obeys domain `singleword`, comment-ban
  and structural limits without exemptions.

## Non-goals

- No LLVM backend dependency.
- No tracing garbage collector.
- No class or struct inheritance hierarchy.
- No source comments, `if` statements (`only-switch`), implicit type coercions
  or multi-word local identifiers in `.sx` source.
- No replacement for domain delivery, Norm or distribution authorities
  (`Concord`, `Plumb`, `Ectropy`, `Wharf`).

## Native records

- Decisions, boundary changes, progress and acceptance live only in
  `PerishLab/simplex` GitHub Issues and pull requests.
- The repository carries no workflow, no skill source and no release notes.

## Verification

- Seed documentation changes are reviewed against
  [GOVERNANCE.md](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
  and [bailiwick.md](https://github.com/PerishLab/.github/blob/main/governance/bailiwick.md).
- From the first implementation Issue onward, every change must pass
  `plumb doctor .`, `ectropy .` and `plumb guard`.

## Exit condition

- **Retirement**: Retire if host languages combined with `Ectropy` and `Plumb`
  satisfy PerishLab's structural clarity, capability isolation and build/link
  latency needs at lower long-term cost than maintaining a dedicated compiler.
- **Split**: If Simplex's non-LLVM backend or `dylib` ABI substrate becomes
  independently consumed outside `.sx` compilation, extract that substrate into
  its own bailiwick.
