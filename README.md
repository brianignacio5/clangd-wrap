# clangd-wrap

Cross-platform wrapper for [clangd](https://clangd.llvm.org/) that proxies LSP traffic over stdio, injects in-memory arguments, watches project configuration files, and restarts clangd when `compile_commands.json` or related config changes.

## Features

- Drop-in replacement for `clangd` in editors (VS Code, Neovim, etc.)
- Transparent pass-through of all clangd CLI arguments
- In-memory argument injection (merged before user args)
- Watches `compile_commands.json`, `build/compile_commands.json`, `compile_flags.txt`, and `.clangd`
- Graceful clangd restart with LSP session replay (`initialize`, open documents)
- Pluggable restart task pipeline

## Environment variables

| Variable | Default | Description |
| --- | --- | --- |
| `CLANGD_PATH` | `clangd` on PATH | Path to the real clangd binary |
| `CLANGD_WRAP_LOG` | `error` | Wrapper log level: `error`, `warn`, `info`, `debug`, `trace` |
| `CLANGD_WRAP_WATCH_ROOT` | current working directory | Root directory for config file discovery and watching |

Wrapper-specific settings use environment variables only so `clangd.path` in editors stays a single binary with no extra flags.

## Argument merging

When spawning clangd, the wrapper merges arguments as:

```
[injected_args...] + [user_args from editor...]
```

`injected_args` are updated by wrapper logic (e.g. reading `.clangd` `CompileFlags.CompilationDatabase`).

## Editor integration

### VS Code

```json
{
  "clangd.path": "C:/path/to/clangd-wrap.exe",
  "clangd.arguments": ["--background-index", "--clang-tidy"]
}
```

### Neovim (lspconfig)

```lua
cmd = { "/path/to/clangd-wrap", "--background-index" }
```

Set `CLANGD_PATH` if clangd is not on your PATH.

## Building

```bash
cargo build --release
```

Release binaries are optimized for size (`lto`, `strip`, `panic = abort`).

## Cross-compilation / releases

GitHub Actions builds release artifacts for:

- `x86_64-pc-windows-msvc` → `clangd-wrap.exe`
- `x86_64-unknown-linux-gnu` → `clangd-wrap`
- `aarch64-apple-darwin` → `clangd-wrap`
- `x86_64-apple-darwin` → `clangd-wrap`

Tag a release (e.g. `v0.1.0`) to trigger the workflow.

## Restart behavior

When a watched config file changes (debounced, content-hash verified):

1. Send LSP `shutdown` / `exit` to clangd
2. Run restart tasks (log, validate CDB, apply `.clangd` config)
3. Respawn clangd with updated arguments
4. Replay `initialize` and `textDocument/didOpen` for tracked buffers

Editors may briefly show stale diagnostics during restart (similar to `clangd.restart` in VS Code).

## Custom restart tasks

Implement the `RestartTask` trait in `src/tasks/mod.rs` and register tasks in `main.rs`. See the default pipeline: `LogChangeTask`, `ValidateCompileCommandsTask`, `ApplyClangdConfigTask`.

## License

MIT
