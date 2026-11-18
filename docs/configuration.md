# App configuration

An app's id and version live in one place: its `Cargo.toml`.

```toml
[package]
name = "my_app"
version = "1.2.3+45"   # version name 1.2.3, build number 45 (`+n` defaults to 1)

[package.metadata.rux]
android-project = "android"
ios-project = "ios"
app-id = "com.example.myapp"           # Android applicationId and iOS bundle identifier
```

## App id and version

`rux build` writes them to per-platform files (gitignored, don't edit them):

| Platform | Generated file | `app-id` | Version name | Build number |
|----------|----------------|----------|--------------|--------------|
| Android | `android/rux.properties` | `applicationId` | `versionName` | `versionCode` |
| iOS | `ios/rux.xcconfig` | `PRODUCT_BUNDLE_IDENTIFIER` | `CFBundleShortVersionString` | `CFBundleVersion` |

- A shared `app-id` must be valid on both platforms: `.`-separated segments starting with a letter,
  using only letters and digits (Android forbids `-`, iOS forbids `_`). Use `android-app-id` /
  `ios-bundle-id` if you need either character.
- The version name must be three numbers (iOS rejects suffixes like `-beta`).
- The build number must be between 1 and 2100000000 (Google Play's limit).

## `[package.metadata.rux]`

| Key | |
|-----|-|
| `android-project` | The Gradle project directory (`rux new`: `android`) |
| `ios-project` | The XcodeGen project directory (`rux new`: `ios`) |
| `app-id` | Application id for both platforms |
| `android-app-id` | Overrides `app-id` on Android |
| `ios-bundle-id` | Overrides `app-id` on iOS |
| `android-ndk-version` | Exact NDK version to build with, installed if missing (default: the newest installed) |
| `ios-development-team` | Apple team to sign device builds with (default: the Mac's only one) |
| `[package.metadata.rux.native]` | Implementations of native APIs (see [Native APIs](native-apis.md)) |
| `[package.metadata.rux.plugin]` | Plugins only: where their Kotlin / Swift sources live |
