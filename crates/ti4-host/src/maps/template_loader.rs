//! Map template loading and management.
//!
//! Two sources: the upstream AsyncTI4 templates mirrored in `ti4-content` (slot layouts whose open
//! slots are filled from seeded filler), and `community_maps.json` next to this file: complete,
//! fixed community layouts with their origin recorded per entry (`source`, `sourceEntry`). See
//! `crates/ti4-server/docs/COMMUNITY_MAPS.md` for how they were converted and checked.

use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

/// A map template definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapTemplate {
    pub alias: String,
    #[serde(default)]
    pub author: String,
    #[serde(rename = "playerCount")]
    pub player_count: usize,
    #[serde(rename = "tilesPerPlayer", default)]
    pub tiles_per_player: usize,
    /// Display name; upstream templates have none and are titled from their alias.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub descr: Option<String>,
    /// Where a community layout was taken from (a URL), and the entry's name there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(
        rename = "sourceEntry",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub source_entry: Option<String>,
    #[serde(rename = "templateTiles")]
    pub template_tiles: Vec<TemplateTile>,
}

/// One position in a template: a fixed tile, a player's home, or a slot to fill.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateTile {
    /// Ring digit then 1-based clockwise index, e.g. `"203"`; `"000"` is the centre.
    pub pos: String,
    #[serde(rename = "staticTileId", default)]
    pub static_tile_id: Option<String>,
    #[serde(rename = "playerNumber", default)]
    pub player_number: Option<usize>,
    #[serde(default)]
    pub home: bool,
}

/// Summary of a map template for listing.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[cfg_attr(feature = "ts", derive(ts_rs::TS))]
pub struct MapTemplateSummary {
    pub alias: String,
    /// Display name, when the template has one (community layouts do).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub author: String,
    pub player_count: usize,
    /// Whether the template builds under the shipped content (always filled in by the server).
    #[serde(default)]
    pub buildable: bool,
    /// Main-map systems (hyperlane tiles excluded); 0 when the template does not build.
    #[serde(default)]
    pub systems: usize,
    #[serde(default)]
    pub hyperlanes: bool,
    /// The template a lobby of this size starts with when nobody chooses.
    #[serde(default)]
    pub recommended: bool,
}

/// Manages loading and listing of map templates.
pub struct TemplateLoader {
    templates: Vec<MapTemplate>,
}

impl TemplateLoader {
    /// Loads all map templates from the embedded content.
    pub fn load() -> Result<Self, String> {
        static TEMPLATES: OnceLock<Result<Vec<MapTemplate>, String>> = OnceLock::new();

        let templates = TEMPLATES
            .get_or_init(|| {
                // Try to load from the relative path to the content directory
                // The path is relative to the workspace root when building
                let json_str = include_str!("../../../ti4-content/content/map_templates.json");
                let mut all: Vec<MapTemplate> = serde_json::from_str(json_str)
                    .map_err(|e| format!("Failed to parse map templates: {}", e))?;
                let community: Vec<MapTemplate> =
                    serde_json::from_str(include_str!("community_maps.json"))
                        .map_err(|e| format!("Failed to parse community maps: {}", e))?;
                for map in community {
                    if all.iter().any(|t| t.alias == map.alias) {
                        return Err(format!("community map alias {:?} is taken", map.alias));
                    }
                    all.push(map);
                }
                Ok(all)
            })
            .clone()?;

        Ok(Self { templates })
    }

    /// Lists all available map templates as summaries.
    pub fn list_summaries(&self) -> Vec<MapTemplateSummary> {
        self.templates
            .iter()
            .map(|t| MapTemplateSummary {
                alias: t.alias.clone(),
                name: t.name.clone(),
                author: t.author.clone(),
                player_count: t.player_count,
                buildable: false,
                systems: 0,
                hyperlanes: false,
                recommended: false,
            })
            .collect()
    }

    /// Gets a template by alias.
    pub fn get(&self, alias: &str) -> Option<&MapTemplate> {
        self.templates.iter().find(|t| t.alias == alias)
    }

    /// Gets the default template for a given player count.
    /// Falls back to procedural generation if no template matches.
    pub fn get_default_for_player_count(&self, player_count: usize) -> Option<&MapTemplate> {
        // Find exact match first
        if let Some(template) = self
            .templates
            .iter()
            .find(|t| t.player_count == player_count)
        {
            return Some(template);
        }

        // No template available for this player count
        None
    }

    /// Every template that seats exactly `player_count`, in file order.
    pub fn templates_for(&self, player_count: usize) -> Vec<&MapTemplate> {
        self.templates
            .iter()
            .filter(|t| t.player_count == player_count)
            .collect()
    }

    /// Checks if a template exists.
    pub fn exists(&self, alias: &str) -> bool {
        self.templates.iter().any(|t| t.alias == alias)
    }

    /// Gets the total number of available templates.
    pub fn count(&self) -> usize {
        self.templates.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_templates() {
        let loader = TemplateLoader::load();
        assert!(loader.is_ok());
    }

    #[test]
    fn test_list_summaries() {
        let loader = TemplateLoader::load().expect("load templates");
        let summaries = loader.list_summaries();
        assert!(!summaries.is_empty());
    }

    #[test]
    fn test_get_template_by_alias() {
        let loader = TemplateLoader::load().expect("load templates");
        let summaries = loader.list_summaries();
        if let Some(first) = summaries.first() {
            let template = loader.get(&first.alias);
            assert!(template.is_some());
        }
    }
}
