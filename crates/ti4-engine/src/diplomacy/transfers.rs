//! Canonical conversion from diplomacy assets to the existing transaction representation.

use ti4_model::{DealTerm, TransferAsset};

use crate::transactions::Terms;

pub(crate) fn immediate_terms(terms: &[DealTerm]) -> Result<Terms, &'static str> {
    let assets = terms.iter().filter_map(|term| match term {
        DealTerm::ImmediateTransfer(asset) => Some(asset),
        _ => None,
    });
    transfer_terms(assets)
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
