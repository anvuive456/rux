# Architecture

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

`App::view()` returns a virtual tree of `Element`s. When a `Signal` changes, rux renders the tree
again, diffs it against the previous one (keyed), and sends the native host only the changes.

All Rust ↔ native communication goes through the `Mutation` enum (create / set_prop / insert_child /
remove_child / release / set_root / listen) and `Event`. Adding a platform means implementing these
operations natively plus a `Renderer` on the Rust side.

## Repository

| Crate / directory | Role |
|-------------------|------|
| `rux` (root) | Facade: re-exports and the `rux::main!` macro that generates the per-platform entry points |
| `crates/rux-core` | `Element`, `Mutation`, `Reconciler` (keyed diffing); platform independent |
| `crates/rux-state` | `Signal<T>`: reactive state that schedules re-renders |
| `crates/rux-ui` | Widgets: `column`, `row`, `view`, `scroll`, `text`, `button`, `text_input` |
| `crates/rux-platform` | `Renderer` trait, `Runtime` (per UI thread), hot reload, `testing::Harness` |
| `crates/rux-android` | Android backend: calls `RuxHost` (Kotlin) over JNI |
| `crates/rux-ios` | iOS backend: calls a C vtable provided by Swift |
| `crates/rux-native` | Calling native code: codec, sync / async calls, test mocks (`rux::native`) |
| `crates/rux-macros` | `#[rux::host_api]`, `#[rux::async_call]`, `#[derive(NativeType)]` |
| `crates/rux-cli` | The `rux` CLI: `setup`, `new`, `doctor`, `build`, `run`; native API codegen; embeds the SDK |
| `platforms/android/rux-runtime` | Android runtime library (Kotlin): `RuxNative`, `RuxHost`, dev client |
| `platforms/ios` | Swift package `RuxRuntime`: `rux.h`, `RuxHost`, dev client |
| `examples/counter` | Sample app with Android and iOS projects |
| `examples/device-info` | Sample plugin: device info (sync) and battery (async) in Kotlin / Swift |

## The SDK and the CLI

The rux SDK is the `rux` crate, the crates it depends on, and the two runtimes. `crates/rux-cli`
embeds it at build time (`build.rs`), and `rux setup` installs it per version in `~/.rux/sdk`; apps
link it as `.rux/sdk`. `rux build` takes the runtimes from the `rux` crate cargo resolved for the
app (written to `android/rux.properties` and `ios/rux.yml`), so they always match the Rust side.
