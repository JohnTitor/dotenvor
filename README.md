# dotenvor

`dotenvor` is a small, fast `.env` parser and loader for Rust.

It focuses on predictable behavior, low dependency overhead, and an ergonomic API (`EnvLoader` + convenience functions).

## Highlights

- Fast parser for common `.env` syntax
- Builder-style loader with multi-file precedence
- Optional variable substitution (`$VAR`, `${VAR}`, `${VAR:-fallback}`)
- Optional upward search for `.env` files
- Process-env or in-memory targets for safer tests
- Quiet/verbose logging controls

## Installation

```toml
[dependencies]
dotenvor = "0.1"
```

## MSRV

The minimum supported Rust version (MSRV) is **Rust 1.88.0+**.
We don't treat MSRV changes as breaking, it may be changed in any release.

## Quick Start

### Load into the process environment

```rust
use dotenvor::dotenv;

// Looks for ".env" and searches parent directories if needed.
let report = unsafe { dotenv()? };
println!("loaded={} skipped={}", report.loaded, report.skipped_existing);
# Ok::<(), dotenvor::Error>(())
```

`dotenv()` mutates process-wide state via `std::env::set_var` and is `unsafe`,
because callers must guarantee no concurrent process-environment access.
In concurrent code or isolated tests, prefer a loader configured with
`.target(TargetEnv::memory())`.

### Builder API with memory target

```rust
use dotenvor::{EnvLoader, KeyParsingMode, SubstitutionMode, TargetEnv};

let mut loader = EnvLoader::new()
    .path(".env")
    .search_upward(true)
    .required(false) // skip missing files instead of returning Error::Io
    .key_parsing_mode(KeyParsingMode::Strict)
    .substitution_mode(SubstitutionMode::Expand)
    .override_existing(false)
    .target(TargetEnv::memory());

let report = loader.load()?;
let env = loader.target_env().as_memory().expect("memory target");

println!("files_read={}", report.files_read);
println!("DATABASE_URL={:?}", env.get("DATABASE_URL"));
# Ok::<(), dotenvor::Error>(())
```

### Parse only

```rust
use dotenvor::parse_str;

let entries = parse_str("A=1\nB=\"hello\"\n")?;
assert_eq!(entries.len(), 2);
# Ok::<(), dotenvor::Error>(())
```

### Opt in to permissive key parsing

```rust
use dotenvor::{parse_str_with_mode, KeyParsingMode};

let entries = parse_str_with_mode(
    "KEYS:CAN:HAVE_COLONS=1\n%TEMP%=/tmp\n",
    KeyParsingMode::Permissive,
)?;
assert_eq!(entries.len(), 2);
# Ok::<(), dotenvor::Error>(())
```

## Implemented Behavior

### Parsing

- `KEY=VALUE` pairs
- Whitespace trimming around keys and values
- Empty values (`FOO=`)
- Comments with `#` outside quotes
- Single quotes, double quotes, and backticks
- Double-quoted escapes: `\n`, `\r`, `\t`, `\\`, `\"`
- Optional `export` prefix
- Duplicate keys: last value wins
- Reader, string, and bytes parsing APIs
- Multiline quoted values (including PEM-style blocks)
- Strict key mode by default, plus opt-in `KeyParsingMode::Permissive`

### Loading

- Multi-file loading with deterministic precedence
- `override_existing(false)` by default
- `EnvLoader` defaults to `TargetEnv::memory()` for process-isolated loads
- Process-env loading is available via `unsafe { TargetEnv::process() }` and
  unsafe convenience functions (`dotenv`, `from_path`, `from_paths`,
  `from_filename`)
- Upward file search support
  - `dotenv()` / `from_filename(...)`: upward search enabled
  - `EnvLoader`: upward search disabled by default (enable with `.search_upward(true)`)
- Missing-file mode
  - `required(true)` (default): missing files return `Error::Io`
  - `required(false)`: missing files are skipped silently

### Substitution

- Optional mode: `SubstitutionMode::Expand`
- Expands `$VAR`, `${VAR}`, and `${VAR:-fallback}` (strict key mode)
- Supports chained and forward references
- Falls back to current target environment values when needed

### Logging

- `.verbose(true)` enables loader diagnostics on stderr
- `.quiet(true)` suppresses diagnostics

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
