//! Loans: the player's loans, as the economy view's loan window lists them,
//! and repaying one.
use crate::{choices::Choice, CountryTag, Date};
use serde::{Deserialize, Serialize};

group! {
    /// The player's loans, and repaying them.
    "loan": LoanRequest, LoanAnswer;

    /// The player's loans, from banks, from other countries and from
    /// estates, each with whether the player can repay it now: the cost is
    /// its amount and the interest left to pay, which the treasury must
    /// hold.
    game List "list" {} -> Vec<PlayerLoan>, "the loans: loan (its ID), lender, estate, amount, interest, expires, can, reasons, cost";

    /// Repays a loan, as its repay button in the loan window does once the
    /// player confirms the price, its amount and the interest left. Refused
    /// with the game's reasons when the treasury holds too little;
    /// `not_found` for an ID that is none of the player's loans.
    game Repay "repay" {
        /// The loan's ID, as `loan.list` gives it.
        loan: u32,
    } -> (), "null once the loan is repaid";
}

/// One of the player's loans.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, crate::shape::Shaped)]
pub struct PlayerLoan {
    /// Its ID, which `loan.repay` names it by.
    pub loan: u32,
    /// The country that lent it; null for a bank's or an estate's.
    pub lender: Option<CountryTag>,
    /// An estate lent it.
    pub estate: bool,
    /// Its ducats.
    pub amount: f64,
    /// Its yearly interest, in percent.
    pub interest: f64,
    /// When it is due.
    pub expires: Date,
    /// Whether the player can repay it now, and what that costs.
    #[serde(flatten)]
    pub repay: Choice,
}
