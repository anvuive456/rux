# Getting started

## Requirements

- [Rust](https://rustup.rs) (rustup). `rux setup` installs the Rust version rux needs.
- For Android: [Android Studio](https://developer.android.com/studio) (it brings the Android SDK
  and a JDK).
- For iOS: a Mac with Xcode and [XcodeGen](https://github.com/yonaskolb/XcodeGen)
  (`brew install xcodegen`).

## Install the `rux` CLI

Download the archive for your platform from the
[latest release](https://github.com/anvuive456/rux/releases/latest), extract it anywhere, and run
its `setup`:

| Platform | Archive |
|----------|---------|
| macOS, Apple silicon | `rux-aarch64-apple-darwin.tar.gz` |
| macOS, Intel | `rux-x86_64-apple-darwin.tar.gz` |
| Linux x64 (static, any distribution) | `rux-x86_64-unknown-linux-musl.tar.gz` |
| Linux arm64 (static, any distribution) | `rux-aarch64-unknown-linux-musl.tar.gz` |
| Windows x64 | `rux-x86_64-pc-windows-msvc.zip` |

```sh
# macOS / Linux
curl -fsSL https://github.com/anvuive456/rux/releases/latest/download/rux-aarch64-apple-darwin.tar.gz | tar -xz
./rux setup
```

```powershell
# Windows
Invoke-WebRequest https://github.com/anvuive456/rux/releases/latest/download/rux-x86_64-pc-windows-msvc.zip -OutFile rux.zip
Expand-Archive rux.zip -DestinationPath rux
.\rux\rux.exe setup
```

Open a new terminal afterwards, so it picks up the new PATH.

Every release lists the archives' checksums in `SHA256SUMS`. On macOS, an archive downloaded with
a browser is quarantined: run `xattr -d com.apple.quarantine rux` before `./rux setup`.

### What `rux setup` does

Like rustup, `rux setup`:

1. **Installs `rux` in `~/.rux/bin`** (the downloaded file can then go) and puts that directory on
   your PATH: the user `Path` on Windows; on macOS and Linux, a line sourcing `~/.rux/env` in
   `~/.profile`, `~/.bashrc`, `~/.zshenv` and `~/.config/fish/conf.d/rux.fish`.
   `--no-modify-path` leaves PATH alone.
2. **Installs the rux SDK** (the rux library of the CLI's version) in `~/.rux/sdk/<version>`. See
   [one version](#one-version-the-cli-and-the-sdk) below.
3. **Installs the Rust tools rux builds with**, if missing: the Rust version rux pins, with the
   Android (and, on macOS, iOS) targets, and `cargo-ndk`. It lists them and asks first; `-y`
   installs without asking.

Set `RUX_HOME` to use another directory than `~/.rux`. Running `rux setup` again is safe: it only
does what's missing.

### Check your setup

```sh
rux doctor
```

It reports the rux SDK, the Rust toolchain and targets, the Android SDK, NDK and JDK, and Xcode,
with a hint for anything missing.

### Build from source

```sh
git clone https://github.com/anvuive456/rux && cd rux
cargo install --path crates/rux-cli
rux setup
```

`rux setup` doesn't copy a `rux` that's already on your PATH (here, in `~/.cargo/bin`).

## Create an app

```sh
rux new my_app
cd my_app
cargo test           # headless UI tests
rux run android      # or: rux run ios
```

Options: `--name <package>` (default: the directory's name) and `--app-id <id>` (default:
`com.example.<name>`; see [App configuration](configuration.md)).

## Project layout

Like a Flutter app: shared Rust code in `src/`, tests in `tests/`, native shells in `android/` and
`ios/`.

```text
my_app/
├── Cargo.toml          # version, app id, and the rux dependency
├── src/
│   ├── lib.rs          # entry point (like main.dart): the root App + rux::main!
│   ├── state/          # app state, one file per piece of state
│   └── views/          # UI, one file per view
├── tests/              # headless UI tests
├── android/            # Gradle project (a shell around the Rust library)
├── ios/                # XcodeGen project (a shell around the Rust library)
└── .rux/sdk            # link to the rux SDK (gitignored, made by `rux setup`)
```

You write the app in `src/`. The native projects only host it: `rux build` keeps the files they
read in sync with `Cargo.toml` (they are gitignored).

## One version: the CLI and the SDK

Like Flutter's SDK, rux isn't published to a package registry. The `rux` CLI carries the rux SDK of
its own version (the `rux` crate, the crates it depends on and the Android / iOS runtimes), so the
tool and the library never drift apart.

`rux setup` installs the SDK in `~/.rux/sdk/<version>` and links it into the app as `.rux/sdk`,
which the app depends on:

```toml
[dependencies]
rux = { version = "=0.2.0", path = ".rux/sdk" }
```

- `rux new` sets this up. After cloning an app, run `rux setup` once in it (like `flutter pub get`).
- `rux build` and `rux run` relink the SDK when it's missing or outdated.
- An app states its rux version: a `rux` CLI of another version refuses to build it, and says which
  release to install, or how to move the app to the CLI's version.

## Next steps

- [Android](android.md) and [iOS](ios.md): running on emulators, simulators and devices.
- [Hot reload](hot-reload.md): what `rux run` does while the app runs.
- [Native APIs & plugins](native-apis.md): calling Kotlin and Swift.
- [Testing](testing.md): UI tests without an emulator.
