# Contributing to rux

Thanks for helping out! Bug reports, ideas and pull requests are welcome.

## Building from source

```sh
git clone https://github.com/anvuive456/rux && cd rux
cargo test --workspace                 # all tests, including the headless UI tests of examples/counter
cargo install --path crates/rux-cli    # the `rux` CLI, with the SDK of this checkout
rux setup
```

The Rust version is pinned in `rust-toolchain.toml` (rustup installs it on first use). See
[Architecture](docs/architecture.md) for how the repository is organized.

To try a change on a device, run the example app from this checkout:

```sh
cd examples/counter
cargo run -p rux-cli -- run android    # or: run ios
```

## Pull requests

`main` is protected: every change lands through a pull request, and CI must pass before merging.

```sh
git switch -c feat/my-feature
# … commit …
git push -u origin feat/my-feature
gh pr create
```

CI runs:

- `cargo fmt --check`, `cargo clippy --workspace --all-targets` and `cargo test --workspace`, with
  warnings as errors;
- the Android build of `examples/counter` (APK) and its iOS build (simulator);
- an app created by `rux new`, outside the checkout, through its tests, its APK and its iOS build.

## Releasing

1. Bump `version` in `[workspace.package]` (root `Cargo.toml`) through a pull request.
2. Once it's merged, tag `main` with the same version and push the tag:

   ```sh
   git switch main && git pull
   git tag v0.2.0 && git push origin v0.2.0
   ```

The [release workflow](.github/workflows/release.yml) checks that the tag matches the version,
builds `rux` (with the SDK) for macOS (arm64, x64), Linux (x64, arm64; static musl) and Windows
(x64), smoke-tests it (`rux setup`, `rux new`, `cargo check` of the app), and publishes a GitHub
Release with the archives, `SHA256SUMS` and generated notes. Tags with a suffix (`v0.2.0-beta.1`)
become pre-releases.

## License

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in
rux by you, as defined in the Apache-2.0 license, shall be dual licensed under the
[Apache-2.0](LICENSE-APACHE) and [MIT](LICENSE-MIT) licenses, without any additional terms or
conditions.
