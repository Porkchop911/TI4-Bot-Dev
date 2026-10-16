//! Map templates (the predefined board layouts and the galaxy built from one) and the catalogue of
//! map choices (what a table can choose, and a picture of each choice), so the server and the
//! browser host offer the same maps.

pub mod build;
pub mod catalog;
pub mod template_loader;

#[cfg(test)]
mod community_tests;

pub use build::{TemplateError, build_template_galaxy, default_template_for, placeholder_homes};
pub use catalog::{MapChoice, MapChoiceView, MapPreview, SeatPreview};
pub use template_loader::{MapTemplate, MapTemplateSummary, TemplateLoader};
