//! Map generation and template management. The templates are in `ti4-view`, the catalogue of
//! choices and previews in `ti4-host`.

pub mod catalog;

pub use ti4_host::maps::{build, template_loader};

#[cfg(test)]
mod community_tests;

pub use ti4_host::maps::{
    MapChoice, MapChoiceView, MapPreview, MapTemplate, MapTemplateSummary, SeatPreview,
    TemplateError, TemplateLoader, build_template_galaxy, default_template_for, placeholder_homes,
};
