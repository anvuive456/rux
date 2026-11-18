<div align="center">

# rux

**Write your app's UI once in Rust. Render it with each platform's native widgets.**

[![CI](https://github.com/anvuive456/rux/actions/workflows/ci.yml/badge.svg)](https://github.com/anvuive456/rux/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/anvuive456/rux?include_prereleases&sort=semver)](https://github.com/anvuive456/rux/releases/latest)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue)](#license)

[Getting started](docs/getting-started.md) ·
[Guides](#documentation) ·
[Examples](examples) ·
[Roadmap](#roadmap) ·
[Contributing](CONTRIBUTING.md)

</div>

> [!WARNING]
> rux is in early development. APIs change between releases, and only the basics (stacks, text,
> buttons, text inputs) are in place. It's ready to experiment with, not to ship apps.

rux is a cross-platform UI framework for Android and iOS. Your app's state and UI live in one Rust
crate; rux turns them into real `android.view.View`s and `UIView`s: no canvas, no WebView, no
custom renderer. Apps look, scroll and behave the way the platform does, and the tooling feels like
Flutter's: one CLI to create, run and hot reload apps.

```rust
use rux::prelude::*;

struct Counter {
    count: Signal<i32>,
}

impl App for Counter {
    fn view(&self) -> Element {
        let count = self.count.clone();
        column([
            text(format!("Count: {}", self.count.get())).font_size(28.0),
            button("+1", move || count.update(|c| *c += 1)),
        ])
        .padding(16.0)
        .spacing(8.0)
    }
}

rux::main!(|| Counter { count: Signal::new(0) });
```

## Why rux?

- **Native widgets.** Text fields, buttons and scroll views are the platform's own, so text
  editing, accessibility and the look and feel come for free.
- **One Rust codebase.** State, logic and UI are shared; the Android and iOS projects are thin
  shells that `rux` generates and keeps in sync.
- **Reactive state.** Update a `Signal` and only what changed is patched in the native view tree.
- **Hot reload.** `rux run` rebuilds on save and swaps the new code into the running app,
  keeping its state.
- **Native APIs without glue code.** Declare an API in Rust; rux generates the Kotlin interface
  and the Swift protocol to implement, and the calls in between.
- **Tested without an emulator.** Headless UI tests click and inspect your UI with `cargo test`.
- **One version.** The `rux` CLI ships the rux library of its own version, so the tool and the
  framework never drift apart.

## Quick start

Install the `rux` CLI from the [latest release](https://github.com/anvuive456/rux/releases/latest)
(macOS, Linux and Windows) and run its setup. It puts `rux` on your PATH and installs the rux
library and the Rust tools it needs (you need [Rust](https://rustup.rs) first):

```sh
# macOS (Apple silicon) — see the release for other platforms
curl -fsSL https://github.com/anvuive456/rux/releases/latest/download/rux-aarch64-apple-darwin.tar.gz | tar -xz
./rux setup
```

```powershell
# Windows
Invoke-WebRequest https://github.com/anvuive456/rux/releases/latest/download/rux-x86_64-pc-windows-msvc.zip -OutFile rux.zip
Expand-Archive rux.zip -DestinationPath rux
.\rux\rux.exe setup
```

Then, in a new terminal:

```sh
rux doctor            # checks Android Studio, Xcode and friends
rux new my_app        # creates my_app/: Rust code, tests, Android and iOS projects
cd my_app
cargo test            # headless UI tests
rux run android       # or: rux run ios — builds, launches, hot reloads on save
```

See [Getting started](docs/getting-started.md) for the details.

## Platforms

| Platform | Status | Renders with | Requires |
|----------|--------|--------------|----------|
| Android  | ✅ Supported (API 24+) | `android.view.View` | Android Studio |
| iOS      | ✅ Supported (iOS 15+) | UIKit | macOS, Xcode, XcodeGen |
| macOS, Windows, Web | 🗓 Planned | AppKit, WinUI, DOM | |

Apps are built from Windows, macOS or Linux; iOS apps need a Mac.

## Documentation

| Guide | |
|-------|-|
| [Getting started](docs/getting-started.md) | Install, create an app, the project layout, the rux SDK |
| [Android](docs/android.md) | Building and running on emulators and devices |
| [iOS](docs/ios.md) | Building and running on simulators and devices, signing |
| [App configuration](docs/configuration.md) | App id, version and `[package.metadata.rux]` |
| [Hot reload](docs/hot-reload.md) | `rux run`, logs, hot reload and hot restart |
| [Native APIs & plugins](docs/native-apis.md) | Calling Kotlin / Swift from Rust, writing plugins |
| [Testing](docs/testing.md) | Headless UI tests, mocking native APIs |
| [Architecture](docs/architecture.md) | How rux works, crate by crate |

## Examples

- [`examples/counter`](examples/counter): a small app with state, several views, headless tests
  and a native API call.
- [`examples/device-info`](examples/device-info): a plugin with a sync and an async native API,
  implemented in Kotlin and Swift.

## Roadmap

- [ ] Layout: flexbox (alignment, weight, sizing); today only stacks with padding and spacing
- [ ] More widgets: image, virtualized lists (RecyclerView / UICollectionView), switch, …
- [ ] Components with local state and fine-grained updates instead of re-rendering `view()`
- [ ] State updates from other threads / async code
- [ ] Batching mutations into one FFI call
- [ ] Hot reload on physical iOS devices (simulators and Android work today)
- [ ] More platforms: macOS (AppKit), Windows (WinUI), Web (DOM)

## Contributing

Contributions are welcome: see [CONTRIBUTING.md](CONTRIBUTING.md) for building rux from source,
running the tests and how changes land.

## License

rux is licensed under either of the [Apache License, Version 2.0](https://www.apache.org/licenses/LICENSE-2.0)
or the [MIT license](https://opensource.org/licenses/MIT), at your option.
