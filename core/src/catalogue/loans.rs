//! Loans: the player's loans, and repaying one.
use eu4_harmony_remote::loans::{LoanAnswer, LoanRequest, PlayerLoan};

layer! {
    run LoanRequest => LoanAnswer;
    List => fn loans() -> Vec<PlayerLoan> = session crate::game::loans::list;
    Repay => fn repay_loan(loan: u32) -> () = session crate::game::loans::repay;
}
