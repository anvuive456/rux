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

`rux setup`:

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
rux new my_app --platforms android,ios
cd my_app
cargo test
rux run android
```

`--platforms` defaults to both platforms. Pass `--platforms ios` or `--platforms android`
for a single platform. `--name <package>` and `--app-id <id>` override the names derived
from the directory.

## Project layout

```text
my_app/
├── Cargo.toml          # Cargo features, app id and version
├── build.rs            # renders native projects for enabled features
├── src/                # shared Rust app
├── tests/              # headless UI tests
├── target/             # Cargo output, including generated native projects
└── .rux/sdk            # link to the installed Rux SDK
```

`rux new` records the selected platforms as default Cargo features. For an iOS-only app:

```toml
[features]
default = ["ios"]
ios = ["rux/ios"]
```

`cargo build` generates the corresponding native project in the app build script's
`OUT_DIR`, typically under `target/.../build/.../out/ios` or `out/android`.
Use `cargo build --no-default-features` for a build with neither native project, or
add `--features ios` to generate only iOS when that feature is available.

`rux build ios` and `rux build android` select a target, wire the native plugins and runtime
into the generated project, and build the Rust library for that platform. They print the
generated project path. `rux run ios` and `rux run android` also build and launch the native app.

## One version: the CLI and the SDK

rux isn't published to a package registry. The `rux` CLI carries the rux SDK of its own version
(the `rux` crate, the crates it depends on and the Android / iOS runtimes), so the tool and the
library never drift apart.

`rux setup` installs the SDK in `~/.rux/sdk/<version>` and links it into the app as `.rux/sdk`,
which the app depends on:

```toml
[dependencies]
rux = { version = "=0.2.0", path = ".rux/sdk" }
```

- `rux new` sets this up. After cloning an app, run `rux setup` once in it.
- `rux build` and `rux run` relink the SDK when it's missing or outdated.
- An app states its rux version: a `rux` CLI of another version refuses to build it, and says which
  release to install, or how to move the app to the CLI's version.

## Next steps

- [Android](android.md) and [iOS](ios.md): running on emulators, simulators and devices.
- [Hot reload](hot-reload.md): what `rux run` does while the app runs.
- [Native APIs & plugins](native-apis.md): calling Kotlin and Swift.
- [Testing](testing.md): UI tests without an emulator.
