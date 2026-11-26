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

The repository includes the complete cold-start specification and prototype
frontend across `M0 · Harness`, `M1 · Surface (lexical, control)`, `M2 · Model
(model, module)` and `M3 · Seams (borrow, async, comptime, dylib)`:

- the governed Rust workspace (`crates/lib` and `crates/cli`, with dependency
  direction `cli -> lib`), `plumb.toml` and `ectropy.toml`;
- the eight-domain pure golden specimen layout under `cases/*` (`lexical`,
  `control`, `model`, `module`, `borrow`, `async`, `comptime`, `dylib`), each
  anchored by a `suite.toml` manifest mapping `reject/*.sx` specimens to exact
  refusal codes so `.sx` files remain completely comment-free; and
- the reviewed `lexical` (6 `pass`, 25 `reject`), `control` (5 `pass`, 21
  `reject`), `model` (5 `pass`, 19 `reject`), `module` (3 `pass`, 18 `reject`),
  `borrow` (4 `pass`, 14 `reject`), `async` (4 `pass`, 12 `reject`), `comptime`
  (4 `pass`, 11 `reject`) and `dylib` (3 `pass`, 11 `reject`) golden suites
  (165 specimens total) paired with zero-backtracking lexical/AST parsing,
  `bucket` + `switch` control laws, unified `:=`
  `struct`/`enum`/`interface`/alias/`import`/`dylib`/`extern "c"` modeling,
  closed `Type : Interface { ... }` conformance blocks, 1:1 file-is-module
  cycle/visibility graph checks, compiler arity/depth/chain caps (`param <= 4`,
  `block <= 4`, `path <= 3`), move-by-default ownership with `Type : Copy {}`
  opt-in, `&T` / `&var T` / `&[T]` / `&var [T]` borrow and slice checking with
  NLL release, first-class native `async` with linear `Fiber<T>` handles and
  `.all().await` / `.race().await` combinators, bounded isomorphic `comptime`
  evaluation, static assertions, `fields`/`variants`/`size` reflection and
  `comptime for` unrolling, `pub dylib` / `leaf := dylib path.leaf` ABI
  monomorphization, transparent-layout and `&Interface` boundary checks,
  generic bound checking, local type inference with zero implicit coercion,
  global single-meaning `_` (only `bucket`'s fallback arm), `uses >= 2` local
  binding hygiene and byte-idempotent canonical formatting in `crates/lib` and
  `simplex {check,format}`.

Non-LLVM native machine-code generation, dynamic linker runtime loading and
standard I/O libraries are not yet implemented; design principles live in
[DESIGN.md](DESIGN.md).

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
- **Orthogonal systems-language scope**: Simplex benchmarks directly against
  C++, Rust and Zig as a systems-grade general-purpose programming language; it
  does not solve markup, layout, styling or other specialized domain-specific
  language (DSL) problems and refuses kitchen-sink scope expansion.
- **Zero silent weakening**: All Rust frontend source obeys domain `singleword`,
  comment-ban and structural limits without exemptions.

## Non-goals

- Solving or absorbing markup, layout, styling or other specialized/future
  domain-specific languages (DSLs).
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
