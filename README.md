# dotenvor

`dotenvor` is a small, fast `.env` parser and loader for Rust.

It focuses on predictable behavior, low dependency overhead, and an ergonomic API (`EnvLoader` + convenience functions).

## Highlights

- Fast parser for common `.env` syntax
- Builder-style loader with multi-file precedence
- Built-in multi-environment stack helper (`.convention("development")`)
- Optional variable substitution (`$VAR`, `${VAR}`, `${VAR:-fallback}`)
- Optional upward search for `.env` files
- First-party `dotenv` CLI (`dotenv run ...`)
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

### Multi-environment stack convention

```rust
use dotenvor::{EnvLoader, TargetEnv};

let mut loader = EnvLoader::new()
    .convention("development")
    .required(false)
    .target(TargetEnv::memory());

loader.load()?;
# Ok::<(), dotenvor::Error>(())
```

Convention precedence (highest to lowest):

- `.env.development.local`
- `.env.local`
- `.env.development`
- `.env`

### Parse only

```rust
use dotenvor::parse_str;

let entries = parse_str("A=1\nB=\"hello\"\n")?;
assert_eq!(entries.len(), 2);
# Ok::<(), dotenvor::Error>(())
```

### CLI: run a command with dotenv files

```bash
cargo run --bin dotenv -- run -- printenv DATABASE_URL
```

Select files explicitly (repeat `-f` or use comma-separated paths):

```bash
cargo run --bin dotenv -- run -f ".env.local,.env" -- my-app
```

Useful flags:

- `-o`, `--override`: let file values override existing environment variables
- `-i`, `--ignore`: skip missing files
- `-u`, `--search-upward`: resolve relative files by walking parent directories

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
- Convention helper for environment stacks (`.convention("development")`)
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
- Configurable file decoding via `.encoding(...)`
  - `Encoding::Utf8` (default)
  - `Encoding::Latin1` (ISO-8859-1)
- CLI command execution (`dotenv run`)
  - Defaults to `.env` when no file is selected
  - Accepts `-f/--file` for file selection (repeatable and comma-separated)
  - Supports `-o/--override` and `-i/--ignore`

### Substitution

- Optional mode: `SubstitutionMode::Expand`
- Expands `$VAR`, `${VAR}`, and `${VAR:-fallback}` (strict key mode)
- Supports chained and forward references
- Falls back to current target environment values when needed
- Treats single-quoted values and escaped dollars (`\$`) as literal in expand mode

### Logging

- `.verbose(true)` enables loader diagnostics on stderr
- `.quiet(true)` suppresses diagnostics

## License

Licensed under either of:

- MIT license ([LICENSE-MIT](LICENSE-MIT))
- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))

at your option.
