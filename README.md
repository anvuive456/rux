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
| `crates/rux-cli`                | The `rux` CLI: `doctor`, `build android`, `build ios` |
| `platforms/android/rux-runtime` | Shared Android library (Kotlin): `RuxNative`, `RuxHost` |
| `platforms/ios`                 | Swift package `RuxRuntime`: `rux.h` + `RuxHost` |
| `examples/counter`              | Sample app + Android / iOS projects |

## App layout

Like Flutter: shared Rust code in `src/`, tests in `tests/`, native shells in `android/` and `ios/`.

```text
examples/counter/
├── Cargo.toml          # [package.metadata.rux] points at android/ and ios/
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

```sh
cd examples/counter
rux build android --abi arm64-v8a --abi x86_64   # → android/app/src/main/jniLibs/<abi>/libcounter.so
```

Open `examples/counter/android` in Android Studio and Run (Android Studio creates the Gradle wrapper on
first open).

### iOS (macOS only)

Requires Xcode, `rustup target add aarch64-apple-ios aarch64-apple-ios-sim`, and `brew install xcodegen`.

```sh
cd examples/counter
rux build ios                           # → target/ios/counter.xcframework
cd ios && xcodegen && open RuxCounter.xcodeproj
```

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
- [ ] `rux new` (project template), `rux run` (build + install + launch)
- [ ] Next platforms: macOS (AppKit), Windows (WinUI), Web (DOM)
