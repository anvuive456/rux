//! Android: `rux build android` and `rux run android`.

mod adb;
mod build;
pub mod java;
mod run;
pub mod sdk;

use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::app::AppCrate;

pub use build::build;
pub use run::run;

/// The Gradle project (`android-project` in `[package.metadata.rux]`).
fn project_dir(app: &AppCrate) -> Result<PathBuf> {
    let project = app
        .meta
        .android_project
        .as_ref()
        .context("missing `android-project` in [package.metadata.rux]")?;
    Ok(app.dir.join(project))
}
