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

### 1. Pure general-purpose surface (never absorbing HTML/CSS)

- **General-purpose across systems, services and UI**: Simplex targets the full
  spectrum from low-level engines and network services to interactive tools and
  interfaces with one unified grammar and semantic model.
- **Zero HTML/CSS dialect absorption**: Simplex will never embed HTML/XML tag
  syntax, JSX/TSX, component template blocks or CSS/stylesheet sub-languages
  (cascading selectors, hyphenated property names or inline style blocks).
  Document markup and cascading style sheets split code into competing scope
  axes and lexical exceptions; in Simplex, UI, graphics and layout are
  constructed purely through ordinary data structures, `interface` contracts,
  functions and `comptime` composition.

### 2. Visual and structural laws by construction

- **`singleword`**: Every locally declared identifier is a single vocabulary
  atom (no underscore-joined or camelCase/PascalCase multi-word compounds).
  Module paths map 1:1 to filesystem paths (file-is-module), without re-export
  barrels or path-alias indirection.
- **`no comment`**: The `.sx` grammar has no line or block comments. Intent that
  cannot be expressed through names, types, exhaustive structure and tests
  belongs in governed design or Issue records, never inline excuses.
- **`no-if, only-switch`**: There is no `if` / `else if` construct. Every
  conditional branch is expressed as a flat, symmetric, exhaustive `switch`
  dispatch, eliminating ad-hoc boolean ladders and hidden fallthroughs.
- **Bounded topology and canonical form**: Compiler limits cap block nesting
  depth and parameter count, refusing anonymous large tuples across boundaries,
  paired with one canonical formatting representation per syntax tree.

### 3. Types, abstraction, memory and control

- **Type inference with explicit boundaries**: Local and expression-level type
  inference eliminates repetitive type noise, while explicit type annotations
  and explicit conversions remain supported and mandatory at conversion edges
  (zero implicit coercion).
- **No inheritance; orthogonal `interface` and generics**: Data declarations
  carry data only. Polymorphism and abstraction use `interface` contracts and
  generics, with zero inheritance hierarchies.
- **Errors as values**: Failure is represented as ordinary typed values
  decomposed through `switch`, with no exception unwinding.
- **Ownership and `borrow` model**: Memory and resource safety follow a
  deterministic ownership and borrow-checking model without a garbage
  collector.
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
