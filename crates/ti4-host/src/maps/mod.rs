//! Map choices and previews. The templates themselves are in `ti4-view`; the catalogue (what a
//! table can choose, and a picture of each choice) is here so the server and the browser host
//! offer the same maps.

pub mod catalog;

pub use ti4_view::maps::{build, template_loader};

pub use build::{TemplateError, build_template_galaxy, default_template_for, placeholder_homes};
pub use catalog::{MapChoice, MapChoiceView, MapPreview, SeatPreview};
pub use template_loader::{MapTemplate, MapTemplateSummary, TemplateLoader};
