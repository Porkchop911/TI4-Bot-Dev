//! Choice system stub.

use ti4_model::*;
use std::collections::{HashMap, BTreeSet};

pub struct Choice {
    pub id: ChoiceId,
    pub player: PlayerId,
    pub options: BTreeSet<OptionId>,
    pub resolved: bool,
    pub resolution: Option<OptionId>,
}

impl Choice {
    pub fn new(id: ChoiceId, player: PlayerId, options: BTreeSet<OptionId>) -> Self {
        Self {
            id,
            player,
            options,
            resolved: false,
            resolution: None,
        }
    }

    pub fn resolve(&mut self, option: OptionId) {
        self.resolved = true;
        self.resolution = Some(option);
    }
}
