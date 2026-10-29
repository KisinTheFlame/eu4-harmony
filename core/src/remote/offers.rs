//! Offers: what other countries proposed to the player, waiting for an
//! answer. Accepting or declining is the player's answer (ADR 0015), given in
//! the offer's dialog.
use super::{tag, Live};
use crate::game::{self, Caller, GResult};
use eu4_harmony_remote::{
    offers::{Offer, OfferAnswer, OfferRequest},
    Date, Error,
};

/// The group's slice of the capability layer.
pub trait Layer {
    fn offers(&mut self) -> GResult<Vec<game::offers::Offer>>;
    fn answer_offer(&mut self, offer: u32, accept: bool) -> GResult<()>;
}

impl Layer for Live {
    fn offers(&mut self) -> GResult<Vec<game::offers::Offer>> {
        game::offers::offers(Caller::Remote)
    }

    fn answer_offer(&mut self, offer: u32, accept: bool) -> GResult<()> {
        game::offers::answer(Caller::Remote, offer, accept)
    }
}

pub fn run(request: OfferRequest, game: &mut (impl Layer + ?Sized)) -> Result<OfferAnswer, Error> {
    Ok(match request {
        OfferRequest::List {} => OfferAnswer::List(
            game.offers()?
                .into_iter()
                .map(|o| Offer { id: o.id, from: tag(o.from), date: Date(o.date), title: o.title, description: o.description })
                .collect(),
        ),
        OfferRequest::Accept { offer } => OfferAnswer::Accept(game.answer_offer(offer, true)?),
        OfferRequest::Decline { offer } => OfferAnswer::Decline(game.answer_offer(offer, false)?),
    })
}
