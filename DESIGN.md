# Design

*Simplex sigillum veri.*

Simplex (`.sx`) is a **general-purpose programming language** conceived for
PerishLab's human-agent collaboration paradigm. Its north star is a single
aesthetic and engineering invariant:

> **Any `.sx` program that passes compilation already looks clean, clear and
> comfortable to read.**

Rather than relying on style guides, linter suppressions or reviewer policing
after the fact, Simplex removes the syntactic and architectural degrees of
freedom through which visual entropy, dialect fragmentation and build latency
accumulate.

## Core principles

### 1. Systems-grade general-purpose scope (refusing kitchen-sink DSLs)

- **Benchmarking C++, Rust and Zig**: Simplex is a pure systems-grade
  general-purpose programming language focused on computation, data
  representation, deterministic memory layout and ownership, control flow,
  concurrency (`async`), compile-time metaprogramming (`comptime`), fast native
  machine-code generation and dynamic library (`dylib`) linking.
- **No markup, layout, styling or specialized DSL scope**: Simplex does not
  solve markup, layout, styling or other specialized/future domain-specific
  language (DSL) problems. It neither absorbs declarative DSL sub-languages
  (such as HTML/XML markup or CSS stylesheets) into `.sx` nor contorts its
  general-purpose core into a kitchen-sink replacement for domain-specific
  declarative tools.

### 2. Visual and structural laws by construction

- **`singleword` and 1:1 file-is-module**: Every locally declared identifier is
  a single vocabulary atom (no underscore-joined or camelCase/PascalCase
  multi-word compounds). Module paths map 1:1 to filesystem paths via
  `leaf := import path.leaf`, with strictly binary file-private vs `pub`
  visibility, zero barrel files (`mod.sx`, `index.sx`, `lib.sx`), zero `pub
  import` re-exports, zero cyclic imports and string-quarantined C FFI bindings
  (`name := extern "c" "symbol" fn(...)`).
- **`no comment`**: The `.sx` grammar has no line or block comments. Intent that
  cannot be expressed through names, types, exhaustive structure and tests
  belongs in governed design or Issue records, never inline excuses.
- **`bucket` + `switch` and first-class functions (`no-if`, `no-while`, `no-return`)**:
  There is no `if`, `while` or `return` construct. Open conditions and
  continuous ranges are partitioned into discrete `enum` states through
  `bucket` (whose final `_ =>` arm is the sole legal meaning of `_` across the
  entire language — value, parameter, payload and `switch` discards are
  forbidden), while `switch` exhaustively dispatches over closed `enum` and
  `bool` values without `_` or fallthrough. Functions are first-class bound
  values: `(params) <switch | bucket | loop>` expresses short `inline` mappings
  with non-`void` inferred results, and `(params) [-> R] { ... }` expresses
  block `fn` contracts.
- **Bounded topology and canonical form**: Compiler limits cap block nesting
  depth (`block <= 4`), parameter count (`param <= 4`) and member access chains
  (`path <= 3`), refusing anonymous tuples across boundaries, paired with one
  canonical formatting representation per syntax tree.

### 3. Types, abstraction, memory and control

- **Type inference with explicit boundaries**: Local and expression-level type
  inference eliminates repetitive type noise, while explicit type annotations
  and explicit conversions remain supported and mandatory at conversion edges
  (zero implicit coercion).
- **No inheritance; orthogonal `interface`, aliases and generics**: Data
  declarations (`struct`, `enum`) carry data only and forbid scattered inherent
  methods (`Type.method`). Methods exist strictly through `interface` contracts
  bound in closed `Type : Interface { ... }` blocks, paired with type aliases
  and generics, with zero inheritance hierarchies.
- **Errors as values**: Failure is represented as ordinary typed values
  decomposed through `switch`, with no exception unwinding.
- **Ownership and low-noise `borrow` model**: Memory and resource safety follow
  a deterministic move-by-default ownership and borrow-checking model without a
  garbage collector or `'a` lifetime sigils. Pure scalar-field value types opt
  into implicit copying via `Type : Copy {}`; call sites distinguish read-only
  borrows (`&x`, `&[T]`), exclusive mutable borrows (`&var x`, `&var [T]`) and
  ownership transfers (`x`) at a glance; first-class borrows are strictly
  lexical views (forbidden inside `struct`/`enum` fields) with last-use NLL
  release and parameter-bound borrow returns.
- **Native `async`**: Asynchronous execution is a first-class language
  construct rather than an external macro or callback rewrite.

### 4. Metaprogramming, compilation and linking

- **Compile-time metaprogramming**: First-class `comptime` / macro-grade
  compile-time reflection and generation, subject to the same structural laws
  so generated and metaprogrammed code never degenerates into an alien dialect.
- **`no llvm`**: Native machine-code generation without LLVM, optimizing for
  sub-second edit-compile-verify turnaround instead of multi-minute backend
  passes and build-lock contention.
- **First-class `dylib`**: Dynamic libraries are a primary modular compilation
  and linking boundary (`interface` + explicit ABI contract), enabling fast
  incremental linking and clean module isolation across bailiwicks.

## Cold-start method

Simplex's syntax and core semantics are locked in through eight orthogonal
`pass / reject` golden specimen suites (`cases/<domain>`) paired with a thin
Rust prototype frontend (`crates/`):

1. Every `.sx` specimen in `pass/` and `reject/` stays pure comment-free `.sx`
   source; expected refusal codes live in `cases/<domain>/suite.toml`.
2. Candidate specimens are verified against unambiguous parsing and canonical
   formatting round-trips alongside human `pass / reject` aesthetic review.
3. Idioms rejected during review as language anti-patterns are promoted into
   `reject/*.sx` as permanent compiler guardrails.
