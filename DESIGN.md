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
  uniform address-stable `Fiber<T>` handles without `Pin<Box<dyn Future>>` type
  erasure, per-fiber `64 KiB` stacks or special concurrency block syntax. Fibers
  and `List<Fiber<T>>` batches compose via postfix `.await`, `.all().await` and
  cancellation-safe `.race().await`, with compile-time linear settlement
  (forbidding detached fibers) and NLL borrow tracking across suspension points.

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

## Target-neutral substrate and tri-OS binary ABI contract

1. **Target-neutral compilation pipeline (`HIR -> LIR -> Target -> Object`)**:
   `simplex-hir` lowers monomorphized `.sx` modules into target-isomorphic
   Block-Arg SSA (`simplex-ir`) preserving aggregate byte layouts, hardware
   memory ordering primitives and abstract linkages (`System`, `Dynamic`);
   `simplex-ir::prepare` lowers `async` functions and `Fiber<T>` operations into
   synchronous state-machine routines; `simplex-target` runs table-driven
   linear-scan register allocation and native instruction encoding; and
   `simplex-object` emits native executables and shared libraries without LLVM,
   external linkers or platform SDKs, unlinking any prior output path before
   writing so OS vnode code-signing caches never observe mutated pages.
2. **Zero-stack address-stable `Fiber<T>` continuation frame**: Each spawned
   fiber allocates a compact heap continuation frame prefixed by a uniform
   cross-`dylib` `FiberHeader` (`step`, `cancel`, `child`, `state`, `size`,
   inline `result` storage and suspension-live locals) stepped directly on the
   host thread stack with zero assembly context-switch stubs; `.all().await`
   polls round-robin and `.race().await` cancels uncompleted losers in reverse
   declaration order.
3. **Deterministic `dylib` mangling**: Exported symbols use deterministic names
   `sx$<ns>$<module>$<name>` and interface dispatch tables use
   `sx$vt$<ns>$<module>$<Type>$<Interface>` (prefixed with `_` in Mach-O symbol
   tables on Apple Silicon).
4. **Tri-OS calling conventions and binary containers**:
   - **`linux-x86_64` (`ELF64` + System V AMD64)**: Up to six GP arguments in
     `rdi, rsi, rdx, rcx, r8, r9` and eight FP arguments in `xmm0..7`; `9..=16 B`
     pairs and aggregates pass in consecutive registers and return in `rax + rdx`;
     aggregates above `16 B` use a hidden `rdi` `sret` pointer; emits standalone
     static `ET_EXEC` when dependency-free, dynamic `PT_INTERP` PIE when linking
     `libc.so.6` or `<stem>.so` (`DT_RUNPATH=$ORIGIN`), and `ET_DYN`
     `<stem>.so` shared libraries.
   - **`windows-x86_64` (`PE32+` + Microsoft x64)**: Up to four arguments in
     `rcx/xmm0, rdx/xmm1, r8/xmm2, r9/xmm3` with mandatory `32 B` caller shadow
     space and `16 B` stack alignment before every call; `1, 2, 4, 8 B` values
     pass in registers while pairs (`16 B`) and all other aggregates pass via
     caller stack-copy pointers and return via hidden `rcx` `sret` pointer;
     links `KERNEL32.dll` (`ExitProcess`), `ucrtbase.dll` and `<stem>.dll` with
     ASCII-sorted `.edata` exports and `.reloc` `DIR64` base relocations.
   - **`darwin-aarch64` (`Mach-O 64` + Apple AAPCS64)**: Up to eight GP
     arguments in `x0..x7` and eight FP arguments in `d0..d7`; `9..=16 B` pairs
     and aggregates pass in consecutive register pairs and return in `x0 + x1`;
     aggregates above `16 B` return via dedicated `x8` indirect result register
     and pass via caller stack-copy pointers; enforces `16 KiB` Mach-O segment
     alignment paired with `4 KiB` `ADRP` page addressing, `LC_UUID`,
     `LC_DYLD_CHAINED_FIXUPS`, `LC_DYLD_EXPORTS_TRIE`
     (`@executable_path/<stem>.dylib`), `/usr/lib/libSystem.B.dylib` and
     built-in SHA-256 `CodeDirectory` (`LC_CODE_SIGNATURE`).

## Library identity, mechanical seal and S3 distribution

1. **Four-tuple Lemma identity (`Coordinate`) and three-tier `x.y.z` law**:
   Every published Lemma release is uniquely addressed by four mandatory
   fields `(scope, name, version, channel)` — where `scope` is a lowercase
   `singleword` and `name` is a 1- or 2-level dot-separated lowercase
   `singleword` (`package` or `package.subpackage`) — with zero defaults and
   zero pre-release or build-metadata version suffixes:
   - **`x` (Architectural Epoch)**: Full paradigm or architectural overhaul
     governed by explicit human authorization (`--epoch`); routine automated
     publishes across `x` fail closed (`0.y.z` carries no special exemption).
   - **`y` (Breaking)**: Any removal or modification of an existing `pub`
     signature, `pub` struct field offset/size/alignment, `pub` enum discriminant
     or variant payload layout, or `pub` interface vtable slot order.
   - **`z` (Compatible)**: Any evolution that preserves 100% of prior `pub`
     declarations and `pub` ABI layouts, covering both private implementation
     edits and purely additive `pub` exports.
   - **`channel`**: First-class exact-match coordinate atom (`stable`, `rc`,
     `canary`, etc.); the compiler enforces exact equality with zero cross-channel
     ranking or fallback, leaving channel admission policy to repository guards.
2. **Lockfile-free manifests (`simplex.lemma/v1`, `simplex.workspace/v1`)**:
   Lemma manifests (`simplex.toml`) declare `[lemma]` (zero `entry` field;
   directory-is-Lemma) and `[dependencies]` using only complete 4-tuple
   coordinates, `{ workspace = "*" }`, or `{ workspace = "pkg.sub" }` workspace
   references, with multi-Lemma workspaces organized under `lemmas/*`. Fuzzy
   version ranges (`^`, `~`, `>=`, `*`), missing `channel` fields, `[library]`,
   `entry`, and any `simplex.lock` file fail closed: because every external edge
   names an immutable SHA-256-sealed 4-tuple coordinate, dependency graphs are
   deterministic without a lockfile. During `seal` and `publish`, workspace
   entries are mechanically rewritten to each workspace member's exact
   `Coordinate`, refusing unpublished workspace dependencies.
3. **Deterministic `source.tar` and mechanical `simplex.seal/v1` audit**:
   Source archives pack the rewritten `simplex.toml`, all `.sx` modules under
   the Lemma directory (zero barrel entry file; directory-wide `pub` surface),
   and Lemma assets in strict lexicographic path order with normalized metadata
   (`mtime=0`, `uid=0`, `gid=0`, `mode=0o644`). `seal.toml` records 64-hex
   SHA-256 digests for `source`, `api` (canonical `pub` surface, invariant to
   private items and function bodies) and `abi` (canonical `pub` struct, enum,
   vtable and `pub dylib` footprints) plus locked dependency digests,
   mechanically auditing `x.y.z` transitions against the channel baseline before
   upload.
4. **Create-Only S3 object store and `<16hex>` `dylib` isolation**:
   Pure-Rust `AWS SigV4` (`AWS4-HMAC-SHA256`) publishes exclusively to
   `<scope>/<name>/<x.y.z>/<channel>/{source.tar, seal.toml}` with
   `If-None-Match: "*"` (`source.tar` first, `seal.toml` commit point; zero
   prebuilt binaries and zero compiler-held credentials, accepting public
   read-only `GET` without credentials). `Store::fetch` verifies coordinate
   equality and `sha256(source.tar) == seal.source` before populating
   `$SIMPLEX_HOME/cache/<scope>/<name>/<x.y.z>/<channel>/` with zero fallback.
   Cross-Lemma HIR derives a 16-hex-char SHA-256 digest (`"simplex.dylib/v1\0"
   + prefix`) to isolate symbols under `sx$<16hex>$<module>$<item>` and
   compiles `dylib` dependencies from verified source into `<16hex>.{so,dll,dylib}`
   shared libraries (zero `lib` prefix) with compile-time hash and output-path
   collision refusal so multiple versions or channels coexist without collision.

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
