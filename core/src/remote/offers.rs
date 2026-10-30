//! Offers: what other countries proposed to the player, waiting for an
//! answer. Accepting or declining is the player's answer (ADR 0015), given in
//! the offer's dialog.
use super::tag;
use crate::game::{self};
use eu4_harmony_remote::{
    offers::{Offer, OfferAnswer, OfferRequest},
    Date, Error,
};

layer! {
    fn offers() -> Vec<game::offers::Offer> = game::offers::offers;
    fn answer_offer(offer: u32, accept: bool) -> () = game::offers::answer;
}

pub fn run(request: OfferRequest, game: &mut (impl Layer + ?Sized)) -> Result<OfferAnswer, Error> {
    Ok(match request {
        OfferRequest::List {} => OfferAnswer::List(
            game.offers()?.into_iter().map(|o| Offer { id: o.id, from: tag(o.from), date: Date(o.date), title: o.title, description: o.description }).collect(),
        ),
        OfferRequest::Accept { offer } => OfferAnswer::Accept(game.answer_offer(offer, true)?),
        OfferRequest::Decline { offer } => OfferAnswer::Decline(game.answer_offer(offer, false)?),
    })
}
