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
  concurrency (`async`), compile-time metaprogramming (`comptime` and `meta`),
  fast native machine-code generation and dynamic library (`dylib`) linking.
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
  cannot be expressed through names, types, exhaustive structure and contracts
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
- **Native `async` and linear `Fiber<T>` concurrency**: Asynchronous functions
  and `interface` methods are first-class values (`async (...) -> T`) producing
  uniform stackful `Fiber<T>` handles without `Pin<Box<dyn Future>>` type
  erasure or special concurrency block syntax. Fibers and `List<Fiber<T>>`
  batches compose via postfix `.await`, `.all().await` and cancellation-safe
  `.race().await`, with compile-time linear settlement (forbidding detached
  fibers) and NLL borrow tracking across suspension points.

### 4. Metaprogramming, compilation, linking and verification

- **Isomorphic `comptime` and typed `@` decorators (no token macros)**:
  Compile-time evaluation (`comptime { ... }`, `comptime` parameters, `assert`,
  `fields<T>()` / `variants<T>()` / `size<T>()` reflection and `comptime for`
  field unrolling) pairs with declaration-level and member-level `@` decorators
  — zero separate `proc-macro` crate or `TokenStream` soup. Function decorators
  wrap synchronous and `async` functions inline without altering their
  signatures; `@derive(...)` synthesizes closed `Type : Interface` conformances
  without mutating transparent `struct` layouts; field tags are inspected via
  `has(field, tag)`; `@caller` injects zero-noise call-site provenance via
  `caller()`; and `embed("...")` confines read-only static assets strictly
  inside the package root. Expression-level AST rewrite macros are forbidden.
- **Zero unit-test syntax and zero `cfg` binary split**: Simplex permanently
  omits built-in unit-test constructs (`test` blocks, `@test`) and conditional
  compilation gates (`@cfg`). Every `.sx` source tree compiles into one exact
  production artifact; local invariants belong in `comptime { assert(...) }`,
  and behavioral contracts are verified exclusively through fast native builds
  paired with isolated black-box process trials (`Seatrial`) and function-level
  causal observation (`Locus`).
- **`no llvm` and first-class `dylib` ABI boundary**: Native machine-code
  generation without LLVM pairs with `pub dylib` / `leaf := dylib path.leaf`
  dynamic library boundaries (zero-`dyn` `&Interface` fat-pointer dispatch,
  library-internal monomorphization and transparent layouts), targeting
  millisecond incremental builds so black-box integration trials run at inner-loop
  speed.

## Cold-start method

Simplex's syntax and core semantics are locked in through nine orthogonal
`pass / reject` golden specimen suites (`cases/<domain>`) paired with a thin
Rust prototype frontend (`crates/`):

1. Every `.sx` specimen in `pass/` and `reject/` stays pure comment-free `.sx`
   source; expected refusal codes live in `cases/<domain>/suite.toml`.
2. Candidate specimens are verified against unambiguous parsing and canonical
   formatting round-trips alongside human `pass / reject` aesthetic review.
3. Idioms rejected during review as language anti-patterns are promoted into
   `reject/*.sx` as permanent compiler guardrails.
