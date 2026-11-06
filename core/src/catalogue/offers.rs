//! Offers: what other countries proposed to the player, waiting for an
//! answer. Accepting or declining is the player's answer
//! (ADR remote-control-uses-capabilities), given in the offer's dialog.
use super::{tag, Convert};
use crate::game::{self};
use eu4_harmony_remote::{
    offers::{Offer, OfferAnswer, OfferRequest},
    Date,
};

layer! {
    run OfferRequest => OfferAnswer;
    List => fn offers() -> Vec<game::offers::Offer> = caller game::offers::offers;
    fn answer_offer(offer: u32, accept: bool) -> () = caller game::offers::answer;
    Accept { offer } => game.answer_offer(offer, true);
    Decline { offer } => game.answer_offer(offer, false);
}

impl Convert<Vec<Offer>> for Vec<game::offers::Offer> {
    fn convert(self) -> Vec<Offer> {
        self.into_iter().map(|o| Offer { id: o.id, from: tag(o.from), date: Date(o.date), title: o.title, description: o.description }).collect()
    }
}
