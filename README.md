# StayUp

StayUp is a Windows desktop app for keeping ordinary programs running in the background. It creates and manages Windows services through the bundled WinSW runtime; it does not scan or modify unrelated Windows services.

## Requirements

- Windows 10 or 11, x64
- Bun 1.4.2
- Rust stable with the MSVC target and Visual Studio C++ build tools
- Microsoft Edge WebView2 Runtime
- .NET Framework 4.6.1 or newer to run the bundled WinSW service wrapper

The installer is a per-machine NSIS package and asks for administrator approval during installation. StayUp itself runs as the signed-in user. Creating, editing, changing startup behavior, and removing a managed service use a one-time UAC helper. Managed programs run as the restricted `LocalService` account.

## Development

```powershell
bun install --frozen-lockfile
bun run winsw:fetch
bun run types:generate
bun run check
bun run test
bun run build
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bun run tauri dev
```

`bun run winsw:fetch` downloads the official WinSW.NET461 2.12.0 release and verifies it against the pinned SHA-256 value in `scripts/winsw-sha256.txt`. The binary is intentionally not committed. `bun run tauri:build` fetches it when needed and creates the Windows installer.

## Architecture

- `crates/stayup-core` contains the application model, validation, generated TypeScript contracts, and WinSW XML adapter.
- `crates/stayup-windows` contains the Windows storage, SCM operations, ACL setup, one-shot elevated helper, logs, and WinSW integration.
- `src-tauri` exposes typed commands to the desktop UI and configures the per-machine NSIS installer.
- `src` contains the React and TypeScript interface.

StayUp's versioned JSON configuration is the source of truth. WinSW XML and per-app wrapper copies are generated files stored under `%ProgramData%\StayUp`. GUI diagnostics belong under `%LOCALAPPDATA%\StayUp`.

## Windows service verification

The automated Rust and frontend tests do not install services or trigger UAC. Validate create, update, lifecycle, restart behavior, ACL isolation, cancellation, upgrade, and uninstall in a disposable Windows 10/11 x64 VM before distributing an installer. Do not test service installation on a daily-use machine.

## WinSW

StayUp bundles the official WinSW.NET461 executable and MIT license. WinSW runs the Windows service protocol and supervises the target process. StayUp owns the user-facing configuration and generates each app's `wrapper.xml`; it does not fork WinSW or treat its XML as the configuration database.
