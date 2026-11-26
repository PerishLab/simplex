# Simplex

Read the canonical [PerishLab delivery governance](https://github.com/PerishLab/.github/blob/main/GOVERNANCE.md)
at work start and again before delivery or Issue closure. That document owns
organization-wide Issue, pull-request, acceptance, version and release policy;
this file keeps repository-specific charter and constraints without copying it.

## Standing

`simplex` is a **candidate bailiwick** in its cold-start specification and
prototype-frontend phase: the explored question, non-goals, native records,
exit condition and governed verification harness exist, while a released
compiler product and Depot `SKILL.md` promise remain `unknown` until code
generation and distribution begin.

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
compile-time evaluation (`comptime`), non-LLVM native code generator and
`dylib` ABI/linking contract.

## Current baseline

This `M0` baseline establishes:

- the governed Rust workspace (`crates/lib` and `crates/cli`, with dependency
  direction `cli -> lib`), `plumb.toml` and `ectropy.toml`;
- the eight-domain pure golden specimen layout under `cases/*` (`lexical`,
  `control`, `model`, `module`, `borrow`, `async`, `comptime`, `dylib`), each
  anchored by a `suite.toml` manifest mapping `reject/*.sx` specimens to exact
  refusal codes so `.sx` files remain completely comment-free; and
- the initial zero-backtracking lexical probe scanner and `simplex check`
  runner verifying `lexical.comment`, `lexical.singleword`, `control.if`,
  `lexical.literal` and `lexical.character` across `cases/*`.

Full parser, canonical formatter, type/borrow checking, `comptime`, `dylib` ABI
checks and native code generation are not yet implemented; target language
principles live in [DESIGN.md](DESIGN.md) and native Issues (`#4`–`#11`).

## Core objects

- **Token**: One lexically classified `.sx` source span produced without
  backtracking or comment channels.
- **Code**: A deterministic law identifier naming the exact syntactic or
  semantic rule that refuses an invalid `.sx` specimen.
- **Suite**: One orthogonal domain directory (`cases/<domain>`) pairing pure
  `pass/*.sx` and `reject/*.sx` specimens with a `suite.toml` refusal manifest.

## Invariants

- **Truth over intention**: Keep implemented truth in `AGENTS.md` and unbuilt
  language design direction in [DESIGN.md](DESIGN.md) and native Issues.
- **Pure `.sx` specimens**: `.sx` files in `cases/*` never carry inline
  expectation comments; every `reject/*.sx` file on disk must be explicitly
  listed with its exact `Code` in that domain's `suite.toml`, and unlisted or
  missing files fail closed.
- **General-purpose purity**: Simplex never absorbs HTML, CSS, JSX/TSX or
  template-markup dialects into its grammar; UI, graphics and layout are
  expressed through ordinary types, functions, interfaces and `comptime`.
- **Zero silent weakening**: All Rust frontend source obeys domain `singleword`,
  comment-ban and structural limits without exemptions.

## Non-goals

- No embedded markup or stylesheet sub-languages (HTML, XML tags, JSX/TSX,
  Svelte/Vue templates or CSS rulesets/hyphenated property syntax).
- No LLVM backend dependency.
- No tracing garbage collector.
- No class or struct inheritance hierarchy.
- No source comments, `if` statements (`only-switch`), implicit type coercions
  or multi-word local identifiers in `.sx` source.
- No replacement for domain delivery, norm or distribution authorities
  (`Concord`, `Plumb`, `Ectropy`, `Wharf`).

## Native records

- Planning lives in [Project #12 (`Simplex · Cold-start`)](https://github.com/orgs/PerishLab/projects/12).
- Decisions, `pass / reject` specimen reviews, boundary changes, progress and
  acceptance live only in `PerishLab/simplex` GitHub Issues and pull requests.
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
