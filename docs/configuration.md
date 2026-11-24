# App configuration

An app's id and version live in one place: its `Cargo.toml`.

```toml
[package]
name = "my_app"
version = "1.2.3+45"   # version name 1.2.3, build number 45 (`+n` defaults to 1)

[package.metadata.rux]
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
| `android-project` | Optional path to a custom Gradle project. New apps generate it in Cargo’s `OUT_DIR`. |
| `ios-project` | Optional path to a custom Xcode project. New apps generate it in Cargo’s `OUT_DIR`. |
| `app-id` | Application id for both platforms |
| `android-app-id` | Overrides `app-id` on Android |
| `ios-bundle-id` | Overrides `app-id` on iOS |
| `android-ndk-version` | Exact NDK version to build with, installed if missing (default: the newest installed) |
| `ios-development-team` | Apple team to sign device builds with (default: the Mac's only one) |
| `[package.metadata.rux.native]` | Implementations of native APIs (see [Native APIs](native-apis.md)) |
| `[package.metadata.rux.plugin]` | Plugins only: where their Kotlin / Swift sources live |

## Native dependencies in `rux.toml`

Declare libraries for generated native projects in `rux.toml` next to `Cargo.toml`.

```toml
[dependencies.android]
okhttp = "com.squareup.okhttp3:okhttp:4.12.0"

[dependencies.ios]
Alamofire = { url = "https://github.com/Alamofire/Alamofire.git", version = "5.9.1", product = "Alamofire" }
```

Android accepts `group:artifact:version` coordinates. Cargo writes them to
`OUT_DIR/android/gradle/libs.versions.toml` using `[versions]`, `[libraries]`, and
`version.ref`. The app module uses the catalog aliases for its `implementation` dependencies.
Maven artifacts resolve through the repositories configured by the generated Gradle project.

On iOS, Cargo adds each Swift package at the exact declared version and links its
product to the generated Xcode app target. Both generated projects follow the enabled
Cargo platform features.
