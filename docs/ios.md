# iOS

iOS apps are built on a Mac.

## Requirements

- Xcode.
- [XcodeGen](https://github.com/yonaskolb/XcodeGen): `brew install xcodegen`. The iOS project is
  described by `ios/project.yml`; the `.xcodeproj` is generated from it (and gitignored).
- The Rust tools `rux setup` installs: the iOS targets.

`rux doctor` shows what's missing.

## Run on a simulator or a device

```sh
rux run ios                          # the booted simulator, else the newest iPhone simulator
rux run ios --device "iPhone 17"     # a simulator by name or id (`xcrun simctl list devices`)
rux run ios --device "My iPhone"     # a connected iPhone / iPad (`xcrun devicectl list devices`)
```

`rux run ios` builds the Rust library and the app, installs and launches it, and stays attached:
it streams the app's output and, on simulators, hot reloads debug builds (see
[Hot reload](hot-reload.md)). It takes the same `--release`, `--no-hot` and `--detach` options as
on Android.

### Signing for devices

Device builds use Xcode's automatic signing. The team is `ios-development-team` in
`[package.metadata.rux]`, else the only team among the Mac's Apple Development certificates:

```toml
[package.metadata.rux]
ios-development-team = "ABCDE12345"
```

## Build only

```sh
rux build ios                        # → target/ios/<name>.xcframework
cd ios && xcodegen && open *.xcodeproj
```

`rux build ios` builds the Rust library for devices and simulators (arm64) into an xcframework, and
writes the files the Xcode project reads (`ios/rux.xcconfig`, `ios/rux.yml`). Run it before
`xcodegen`.

The xcframework holds one profile at a time: `rux build ios` for the Debug configuration,
`rux build ios --release` for Release (archives for TestFlight / the App Store). The project's
"Check rux library" build phase fails the build if they don't match, so a release never ships the
debug library.
