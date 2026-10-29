//! Offers (GLOSSARY): what other countries proposed to the player that waits
//! for the player to accept or decline, and answering one as the player does
//! in its dialog.
use crate::{CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// What other countries proposed to the player, waiting for an answer.
    "offer": OfferRequest, OfferAnswer;

    /// The offers waiting for the player's answer, shown as diplomatic
    /// message icons or open dialogs. Texts are as the game shows them, in
    /// plain text: no color codes, icons as their names. An offer left
    /// unanswered for about two months is declined by the game.
    game List "list" {} -> Vec<Offer>, "the offers: id, from, date, title, description";
    /// Accepts an offer, as the player does in its dialog. Done means the
    /// game has the answer; it applies it a moment later, and the offer is
    /// gone from the list at once.
    game Accept "accept" {
        /// The offer's id, from offer.list.
        offer: u32,
    } -> (), "nothing";
    /// Declines an offer, as the player does in its dialog. Done means the
    /// game has the answer; it applies it a moment later, and the offer is
    /// gone from the list at once.
    game Decline "decline" {
        /// The offer's id, from offer.list.
        offer: u32,
    } -> (), "nothing";
}

/// A proposal waiting for the player's answer.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Offer {
    /// Names the offer while it waits.
    pub id: u32,
    /// The country that made it.
    pub from: CountryTag,
    /// When it was made.
    pub date: Date,
    /// What is offered, such as "Royal Marriage".
    pub title: String,
    pub description: String,
}
