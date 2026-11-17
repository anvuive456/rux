//! iOS: `rux build ios` and `rux run ios`.

mod build;
mod run;
mod simctl;

use std::path::PathBuf;

use crate::app::AppCrate;

pub use build::build;
pub use run::run;

/// The Xcode / XcodeGen project (`ios-project` in `[package.metadata.rux]`), if configured.
fn project_dir(app: &AppCrate) -> Option<PathBuf> {
    app.meta.ios_project.as_ref().map(|p| app.dir.join(p))
}
