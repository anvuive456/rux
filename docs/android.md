# Android

## Requirements

- [Android Studio](https://developer.android.com/studio), or the Android SDK on its own.
- The Rust tools `rux setup` installs: the Android targets and `cargo-ndk`.

No environment variables are needed:

- **Android SDK:** found in Android Studio's default location (`~/Library/Android/sdk`,
  `~/Android/Sdk`, `%LOCALAPPDATA%\Android\Sdk`). Set `ANDROID_HOME` if yours is elsewhere.
- **NDK** (needed to build Rust for Android): you don't install it yourself. `rux` uses the
  newest NDK in your SDK, and if there is none, installs a pinned version on the first build,
  through the SDK's command-line tools. Pin an exact version with `android-ndk-version = "…"` in
  `[package.metadata.rux]`, or point `ANDROID_NDK_HOME` at an NDK.
- **JDK:** Gradle and the SDK tools run on `JAVA_HOME`, else `java` on PATH, else the JDK bundled
  with Android Studio. Gradle needs Java 17 or newer.
- **Gradle:** each generated Android project includes a Gradle wrapper (`gradlew`), which downloads its pinned Gradle
  version once into `~/.gradle`. `rux build android` writes `sdk.dir` into `local.properties`.

`rux doctor` shows what's found and where.

## Run on an emulator or a device

Start an emulator (Android Studio > Device Manager) or connect a device with USB debugging, then:

```sh
rux run android
```

`rux run android` builds the Rust library for the device's ABI only, builds and installs the APK,
launches the app, and stays attached: it streams the app's logs and hot reloads debug builds (see
[Hot reload](hot-reload.md)).

| Option | |
|--------|-|
| `--device <serial>` | Pick a device when several are connected (see `adb devices`) |
| `--release` | Release build (no hot reload) |
| `--abi <abi>` | Build for this ABI instead of the device's |
| `--no-hot` | Stay attached (logs) without hot reload |
| `--detach` | Just launch the app and exit |

## Build only

```sh
rux build android                          # arm64-v8a
rux build android --abi arm64-v8a --abi x86_64
```

This copies `lib<name>.so` into `app/src/debug/jniLibs/<abi>/` in the Android project.
Release builds use `app/src/release/jniLibs/<abi>/`. The command also writes
`rux.properties` and `local.properties` in the project.

For apps created by `rux new`, the project lives under Cargo's `OUT_DIR/android`. Open the
project directory printed by the command in Android Studio, or run the wrapper from that directory.

```sh
./gradlew assembleDebug                    # Windows: gradlew.bat assembleDebug
```

The ABIs in `app/build.gradle.kts` (`abiFilters`) must match the ones you build.
