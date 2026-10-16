//! Print every feature name of the card-text families, one per line, sorted.
//!
//! The list `migrate_bundle_append_names --grow` appends to a checkpoint's vocabulary so that the
//! bot enables `card-tag:*` / `card-tag-opt:*` (see `ti4_policy::projection::card_tags_enabled_for`):
//!
//! ```text
//! cargo run -p ti4-policy --example card_tag_names > names.txt
//! ```

fn main() {
    for name in ti4_policy::projection::card_tag_names() {
        println!("{name}");
    }
}
