//! Canonical conversion from diplomacy assets to the existing transaction representation.

use ti4_model::{DealTerm, TransferAsset};

use crate::transactions::Terms;

type CountedAsset = fn(u8) -> TransferAsset;

pub(crate) fn immediate_terms(terms: &[DealTerm]) -> Result<Terms, &'static str> {
    let assets = terms.iter().filter_map(|term| match term {
        DealTerm::ImmediateTransfer(asset) => Some(asset),
        _ => None,
    });
    transfer_terms(assets)
}

/// The reverse of [`immediate_terms`]: one side of a transaction as bundle assets.
///
/// `None` when an amount does not fit a bundle term or a fragment trait is unknown, so a shape the
/// bundle vocabulary cannot carry is left out rather than silently changed.
pub(crate) fn assets_of(terms: &Terms) -> Option<Vec<TransferAsset>> {
    let amount = |n: i32| u8::try_from(n).ok();
    let mut out = Vec::new();
    if terms.trade_goods > 0 {
        out.push(TransferAsset::TradeGoods(amount(terms.trade_goods)?));
    }
    if terms.commodities > 0 {
        out.push(TransferAsset::Commodities(amount(terms.commodities)?));
    }
    let traits: [(&str, CountedAsset); 4] = [
        ("cultural", TransferAsset::CulturalFragments),
        ("hazardous", TransferAsset::HazardousFragments),
        ("industrial", TransferAsset::IndustrialFragments),
        ("unknown", TransferAsset::UnknownFragments),
    ];
    if terms
        .fragments
        .iter()
        .any(|fragment| !traits.iter().any(|(name, _)| fragment == name))
    {
        return None;
    }
    for (name, make) in traits {
        let count = terms
            .fragments
            .iter()
            .filter(|fragment| fragment.as_str() == name)
            .count();
        if count > 0 {
            out.push(make(u8::try_from(count).ok()?));
        }
    }
    if let Some(note) = &terms.promissory {
        out.push(TransferAsset::PromissoryNote(note.clone()));
    }
    if let Some(card) = &terms.action_card {
        out.push(TransferAsset::ActionCard(card.clone()));
    }
    if let Some(secret) = &terms.secret {
        out.push(TransferAsset::SecretObjective(secret.to_string()));
    }
    Some(out)
}

pub(crate) fn one_asset(asset: &TransferAsset) -> Result<Terms, &'static str> {
    transfer_terms(std::iter::once(asset))
}

fn transfer_terms<'a>(
    assets: impl Iterator<Item = &'a TransferAsset>,
) -> Result<Terms, &'static str> {
    let mut out = Terms::default();
    for asset in assets {
        match asset {
            TransferAsset::TradeGoods(n) => out.trade_goods += i32::from(*n),
            TransferAsset::Commodities(n) => out.commodities += i32::from(*n),
            TransferAsset::CulturalFragments(n) => out
                .fragments
                .extend(std::iter::repeat_n("cultural".to_owned(), usize::from(*n))),
            TransferAsset::HazardousFragments(n) => out
                .fragments
                .extend(std::iter::repeat_n("hazardous".to_owned(), usize::from(*n))),
            TransferAsset::IndustrialFragments(n) => out.fragments.extend(std::iter::repeat_n(
                "industrial".to_owned(),
                usize::from(*n),
            )),
            TransferAsset::UnknownFragments(n) => out
                .fragments
                .extend(std::iter::repeat_n("unknown".to_owned(), usize::from(*n))),
            TransferAsset::PromissoryNote(id) if out.promissory.is_none() => {
                out.promissory = Some(id.clone());
            }
            TransferAsset::ActionCard(id) if out.action_card.is_none() => {
                out.action_card = Some(id.clone());
            }
            TransferAsset::SecretObjective(id) if out.secret.is_none() => {
                out.secret = Some(ti4_model::SecretObjectiveId::new(id));
            }
            _ => return Err("a transaction can carry at most one card of each supported kind"),
        }
    }
    Ok(out)
}
