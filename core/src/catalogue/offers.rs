//! Offers: what other countries proposed to the player, waiting for an
//! answer. Accepting or declining is the player's answer
//! (ADR remote-control-uses-capabilities), given in the offer's dialog.
use eu4_harmony_remote::offers::args;
use eu4_harmony_remote::offers::{Offer, OfferAnswer, OfferRequest};

layer! {
    run OfferRequest => OfferAnswer;
    List => fn offers -> Vec<Offer> = crate::game::offers::offers;
    Accept => fn accept_offer -> () = crate::game::offers::accept;
    Decline => fn decline_offer -> () = crate::game::offers::decline;
}
