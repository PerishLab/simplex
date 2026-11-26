# Design

*Simplex sigillum veri.*

Simplex (`.sx`) is a programming language conceived for PerishLab's human-agent
collaboration paradigm. Its north star is a single aesthetic and engineering
invariant:

> **Any `.sx` program that passes compilation already looks clean, clear and
> comfortable to read.**

Rather than relying on style guides, linter suppressions or reviewer policing
after the fact, Simplex removes the syntactic and architectural degrees of
freedom through which visual entropy and build latency accumulate.

## Core principles

### 1. Visual and structural laws by construction

- **`singleword`**: Every locally declared identifier is a single vocabulary
  atom. Module paths map 1:1 to filesystem paths (file-is-module), without
  re-export barrels or path-alias indirection.
- **`no comment`**: The `.sx` grammar has no line or block comments. Intent that
  cannot be expressed through names, types, exhaustive structure and tests
  belongs in governed design or Issue records, never inline excuses.
- **`no-if, only-switch`**: There is no `if` / `else if` construct. Every
  conditional branch is expressed as a flat, symmetric, exhaustive `switch`
  dispatch, eliminating ad-hoc boolean ladders and hidden fallthroughs.
- **Bounded topology and canonical form**: Compiler limits cap block nesting
  depth and parameter count, refusing anonymous large tuples across boundaries,
  paired with one canonical formatting representation per syntax tree.

### 2. Types, abstraction, memory and control

- **Type inference with explicit boundaries**: Local and expression-level type
  inference eliminates repetitive type noise, while explicit type annotations
  and explicit conversions remain supported and mandatory at conversion edges
  (zero implicit coercion).
- **No inheritance; orthogonal `interface` and generics**: Data declarations carry
  data only. Polymorphism and abstraction use `interface` contracts and
  generics, with zero inheritance hierarchies.
- **Errors as values**: Failure is represented as ordinary typed values
  decomposed through `switch`, with no exception unwinding.
- **Ownership and `borrow` model**: Memory and resource safety follow a
  deterministic ownership and borrow-checking model without a garbage
  collector.
- **Native `async`**: Asynchronous execution is a first-class language
  construct rather than an external macro or callback rewrite.

### 3. Metaprogramming, compilation and linking

- **Compile-time metaprogramming**: First-class `comptime` / macro-grade
  compile-time reflection and generation, subject to the same structural laws
  so generated and metaprogrammed code never degenerates into an alien dialect.
- **`no llvm`**: Native machine-code generation without LLVM, optimizing for
  sub-second edit-compile-verify turnaround instead of multi-minute backend
  passes and build-lock contention.
- **First-class `dylib`**: Dynamic libraries are a primary modular compilation
  and linking boundary (`interface` + explicit ABI contract), enabling fast
  incremental linking and clean module isolation across bailiwicks.

## Growth strategy

Simplex grows deliberately from real domain pressure rather than speculative
feature accumulation. Open design trade-offs—such as exact `comptime` vs.
hygienic AST macro boundaries, `borrow` rules across `dylib` interfaces, and
native backend target staging—are explored and settled one bounded Issue at a
time in `PerishLab/simplex`.
