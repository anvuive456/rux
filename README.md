# rux

[![CI](https://github.com/anvuive456/rux/actions/workflows/ci.yml/badge.svg)](https://github.com/anvuive456/rux/actions/workflows/ci.yml)

A cross-platform UI framework: **write your UI once in Rust, render it with native widgets** on each
platform (Android `View`, iOS `UIKit`). There is no custom canvas/GPU renderer — apps look and behave
like native apps.

```rust
use rux::prelude::*;

struct Counter { count: Signal<i32> }

impl App for Counter {
    fn view(&self) -> Element {
        let inc = self.count.clone();
        column([
            text(format!("Count: {}", self.count.get())).font_size(28.0),
            button("+", move || inc.update(|c| *c += 1)),
        ])
        .padding(16.0)
    }
}

rux::main!(|| Counter { count: Signal::new(0) });
```

## Architecture

```text
          Rust (single source)                                Native host
┌──────────────────────────────────────────┐         ┌───────────────────────────────┐
│ App::view() → Element (virtual tree)     │         │                               │
│        │                                 │         │ Android: RuxHost.kt (View)    │
│        ▼                                 │Mutation │ iOS:     RuxHost.swift (UIKit)│
│ Reconciler (diff) → Vec<Mutation> ───────┼────────►│                               │
│        ▲                                 │         │                               │
│ Signal::set ◄─ handler ◄─ dispatch ◄─────┼─────────┤ click / change                │
└──────────────────────────────────────────┘  Event  └───────────────────────────────┘
```

All Rust ↔ native communication goes through the `Mutation` enum (create / set_prop / insert_child /
remove_child / release / set_root / listen) and `Event`. Adding a platform means implementing these
9 operations natively plus a `Renderer` on the Rust side.

| Crate / directory               | Role |
|---------------------------------|------|
| `rux` (root)                    | Facade: re-exports + the `rux::main!` macro that generates per-platform entry points |
| `crates/rux-core`               | `Element`, `Mutation`, `Reconciler` (keyed diffing) — platform independent |
| `crates/rux-state`              | `Signal<T>` — reactive state that schedules re-renders |
| `crates/rux-ui`                 | Widgets: `column`, `row`, `view`, `scroll`, `text`, `button`, `text_input` |
| `crates/rux-platform`           | `Renderer` trait, `Runtime`, per-UI-thread runtime, `testing::Harness` (headless) |
| `crates/rux-android`            | Android backend: calls `RuxHost` (Kotlin) over JNI |
| `crates/rux-ios`                | iOS backend: calls a C vtable provided by Swift |
| `crates/rux-cli`                | The `rux` CLI: `doctor`, `build`, `run` (android / ios) |
| `platforms/android/rux-runtime` | Shared Android library (Kotlin): `RuxNative`, `RuxHost` |
| `platforms/ios`                 | Swift package `RuxRuntime`: `rux.h` + `RuxHost` |
| `examples/counter`              | Sample app + Android / iOS projects |

## App layout

Like Flutter: shared Rust code in `src/`, tests in `tests/`, native shells in `android/` and `ios/`.

```text
examples/counter/
├── Cargo.toml          # version = "x.y.z+n"; [package.metadata.rux]: app-id, android/ and ios/ paths
├── src/
│   ├── lib.rs          # entry point (like main.dart): root App + rux::main!
│   ├── state/          # one file per piece of state
│   │   ├── counter.rs
│   │   └── profile.rs
│   └── views/          # one file per view
│       ├── home.rs
│       ├── counter_view.rs
│       └── greeting_view.rs
├── tests/              # headless UI tests
│   ├── counter_test.rs
│   └── greeting_test.rs
├── android/            # Gradle project (shell only)
└── ios/                # XcodeGen project (shell only)
```

## Getting started

```sh
cargo test --workspace                 # runs all tests, including the headless UI tests in examples/counter
cargo install --path crates/rux-cli    # installs the `rux` CLI
rux doctor                             # checks your toolchains
```

### Android

Requires the Android SDK + NDK, `cargo install cargo-ndk`, and
`rustup target add aarch64-linux-android x86_64-linux-android`.
Building Rust for Android needs the NDK (a separate package from the SDK). Like Flutter's
`ndkVersion`, you don't install it yourself: `rux` uses the newest NDK in your SDK, and if there is
none, installs a pinned version on the first `rux build android` (via the SDK's command-line tools).
Pin an exact version with `android-ndk-version = "…"` in `[package.metadata.rux]`, or point
`ANDROID_NDK_HOME` at an NDK to override.

```sh
cd examples/counter
rux build android --abi arm64-v8a --abi x86_64   # → android/app/src/main/jniLibs/<abi>/libcounter.so
```

Open `examples/counter/android` in Android Studio and Run, or build + install + launch from the
terminal on a running emulator / connected device:

```sh
rux run android                 # only builds the device's ABI; --device <serial> if several are connected
```

The Android project ships a Gradle wrapper (`gradlew`, pinned Gradle version, reused from
`~/.gradle` if already downloaded), and `rux build android` writes `sdk.dir` into
`local.properties`, so neither Gradle nor `ANDROID_HOME` has to be set up by hand.

### iOS (macOS only)

Requires Xcode, `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, and `brew install xcodegen`.

```sh
cd examples/counter
rux build ios                           # → target/ios/counter.xcframework
cd ios && xcodegen && open RuxCounter.xcodeproj
```

Or build + install + launch on a simulator (the booted one, else the newest iPhone; `--device <name or
UDID>` to choose):

```sh
rux run ios
```

### App id & version

Like Flutter's `pubspec.yaml`, the application id and version live in one place — the app's
`Cargo.toml`:

```toml
[package]
version = "1.2.3+45"   # version name 1.2.3, build number 45 (`+n` defaults to 1)

[package.metadata.rux]
app-id = "com.example.myapp"           # Android applicationId + iOS bundle identifier
# android-app-id = "com.example.myapp" # optional per-platform overrides
# ios-bundle-id = "com.example.myapp"
```

`rux build` generates the per-platform files from it (they are gitignored, don't edit them):

| Platform | Generated file           | `app-id`                    | Version name                 | Build number      |
|----------|--------------------------|-----------------------------|------------------------------|-------------------|
| Android  | `android/rux.properties` | `applicationId`             | `versionName`                | `versionCode`     |
| iOS      | `ios/rux.xcconfig`       | `PRODUCT_BUNDLE_IDENTIFIER` | `CFBundleShortVersionString` | `CFBundleVersion` |

A shared `app-id` must be valid on both platforms: `.`-separated segments starting with a letter,
using only letters and digits (Android forbids `-`, iOS forbids `_`). Use the per-platform keys if
you need either character.

The version name must be three numbers (iOS rejects suffixes like `-beta`); the build number must be
between 1 and 2100000000 (Google Play's limit).

## Hot reload

`rux run` (debug builds) stays attached to the app, like `flutter run`:

```text
🔥 Hot reload is on: save a file to hot reload.
   r ⏎  hot reload (keep state)   R ⏎  hot restart   q ⏎  quit
✓ hot reload: kept 2 state value(s) (691 ms)
```

- **Save a file / `r`: hot reload.** Only the app library is rebuilt; the running app loads it
  (`dlopen`) without being reinstalled or relaunched, and keeps its state: every `Signal` created
  while constructing the app is handed over to the new build, matched by creation order and type.
- **`R`: hot restart.** Same, but the app is constructed from scratch.
- If a `struct` / `enum` definition (or `Cargo.toml`) changed, or the app's signals no longer
  match (added, removed, retyped), `rux run` does a hot restart instead and says why.
- A build error is reported and the app keeps running; save again once it's fixed.

How it works: the library the app starts with acts as a trampoline. The app's dev client
(`RuxDevClient` in the Android / iOS runtimes, debug builds only) connects to the dev server of
`rux run` (through `adb reverse` on Android), receives each new build, and the trampoline hands the
app over to it (`rux_platform::dev`). Old builds stay loaded, so state created by them stays valid.
Pass `--no-hot` to just launch the app. iOS hot reload works on the simulator.

## UI tests without an emulator

```rust
let mut h = rux::testing::Harness::new(CounterApp::new());
h.click_text("+");
assert!(h.dump().contains(r#"text="Count: 1""#));
```

## Contributing

`main` is protected: every change lands through a pull request, and CI (Rust, Android, iOS) must pass
before merging.

```sh
git switch -c feat/my-feature
# ...commit...
git push -u origin feat/my-feature
gh pr create
```

## Roadmap

- [ ] Layout: flexbox (alignment, weight, sizing) — currently only stacks + padding/spacing
- [ ] More widgets: image, virtualized list (RecyclerView / UICollectionView), switch, ...
- [ ] Components with local state & fine-grained updates instead of re-rendering the whole `view()`
- [ ] State updates from other threads / async (marshalled to the UI thread)
- [ ] Batch mutations into a single FFI call (fewer JNI crossings)
- [ ] `rux new` (project template)
- [ ] `rux run`: stream app logs, physical iOS devices (code signing), single-key commands
- [ ] Next platforms: macOS (AppKit), Windows (WinUI), Web (DOM)
