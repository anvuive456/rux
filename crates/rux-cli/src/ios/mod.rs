//! iOS: `rux build ios` and `rux run ios`.

mod build;
mod devicectl;
mod run;
mod signing;
mod simctl;

pub use build::build;
pub use run::run;
