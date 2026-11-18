# Hot reload

`rux run` stays attached to the app, like `flutter run`. It streams the app's logs (Android: logcat
of the app's process, including Rust's stdout / stderr; iOS: the app's stdout / stderr) and reads
single-key commands. In debug builds it hot reloads:

```text
🔥 Hot reload is on: save a file to hot reload.
   r  hot reload (keep state)   R  hot restart   c  clear   h  help   q  quit (the app keeps running)
✓ hot reload: kept 2 state value(s) (691 ms)
```

- **Save a file, or `r`: hot reload.** Only the app library is rebuilt. The running app loads it
  without being reinstalled or relaunched, and keeps its state: every `Signal` created while
  constructing the app is handed over to the new build, matched by creation order and type.
- **`R`: hot restart.** The same, but the app is constructed from scratch.
- If a `struct` / `enum` definition (or `Cargo.toml`) changed, or the app's signals no longer match
  (added, removed, retyped), `rux run` does a hot restart instead and says why.
- A build error is reported and the app keeps running; save again once it's fixed.
- Changing a `#[rux::host_api]` declaration needs a full `rux run` (hot reload warns about it).

Without a terminal (e.g. piped output), commands are read line by line. `--no-hot` stays attached
without hot reload; `--detach` just launches the app.

Hot reload works on Android emulators and devices, and on iOS simulators. Physical iOS devices get
logs only.

## How it works

The library the app starts with acts as a trampoline. In debug builds, the app's dev client
(`RuxDevClient` in the Android / iOS runtimes) connects to the dev server of `rux run` (through
`adb reverse` on Android), receives each new build, and the trampoline hands the app over to it
(`dlopen`; see `rux_platform::dev`). Old builds stay loaded, so state created by them stays valid.
