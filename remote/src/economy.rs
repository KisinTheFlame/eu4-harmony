//! The economy: what the player spends monarch points on for the treasury,
//! reducing inflation and raising mercantilism, and taking a bank loan, as
//! the economy view's buttons and the trade view's mercantilism button do.
use crate::choices::{Amount, Choice};
use serde::{Deserialize, Serialize};

group! {
    /// The treasury's monarch-point spending and bank loans.
    "economy": EconomyRequest, EconomyAnswer;

    /// What the buttons offer now: taking a bank loan, reducing inflation
    /// and raising mercantilism, each with whether the player can, why not
    /// as the button's tooltip says, and its cost; a loan's ducats are in
    /// `gives`, which follow the income the game estimates: what changes the
    /// income, such as developing a province, changes them at once.
    game Options "options" {} -> Vec<EconomyOption>, "one for each of take_loan, reduce_inflation, raise_mercantilism: action, can, reasons, cost, gives";

    /// Takes a bank loan of the size the economy view offers now, as its
    /// button does: the ducats `economy.options` gives, when nothing changed
    /// the income since. Refused with the game's reasons when the player cannot take one
    /// now; the loan is then in `loan.list`.
    game TakeLoan "take_loan" {} -> (), "null once the loan is taken";

    /// Reduces inflation with administrative points, as the economy view's
    /// button does. Refused with the game's reasons when the player cannot
    /// now.
    game ReduceInflation "reduce_inflation" {} -> (), "null once inflation is reduced";

    /// Raises mercantilism with diplomatic points, as the trade view's
    /// button does. Refused with the game's reasons when the player cannot
    /// now.
    game RaiseMercantilism "raise_mercantilism" {} -> (), "null once mercantilism is raised";
}

/// What the economy's buttons let the player do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, crate::shape::Shaped)]
#[serde(rename_all = "snake_case")]
pub enum EconomyAction {
    /// Taking a bank loan.
    TakeLoan,
    /// Reducing inflation with administrative points.
    ReduceInflation,
    /// Raising mercantilism with diplomatic points.
    RaiseMercantilism,
}

/// One thing the economy's buttons let the player do, and whether it can
/// now.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct EconomyOption {
    pub action: EconomyAction,
    #[serde(flatten)]
    pub choice: Choice,
    /// What it gives the treasury: the loan's ducats for take_loan; empty
    /// for the others.
    pub gives: Vec<Amount>,
}
