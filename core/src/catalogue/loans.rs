//! Loans: the player's loans, and repaying one.
use eu4_harmony_remote::loans::{LoanAnswer, LoanRequest, PlayerLoan};
use eu4_harmony_remote::loans::args;

layer! {
    run LoanRequest => LoanAnswer;
    List => fn loans -> Vec<PlayerLoan> = crate::game::loans::list;
    Repay => fn repay_loan -> () = crate::game::loans::repay;
}
