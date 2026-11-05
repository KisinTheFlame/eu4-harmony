//! Loans: the player's loans, and repaying one.
use eu4_harmony_remote::loans::{Loan, LoanAnswer, LoanRequest};

layer! {
    run LoanRequest => LoanAnswer;
    List => fn loans() -> Vec<Loan> = crate::game::loans::list;
    Repay => fn repay_loan(loan: u32) -> () = crate::game::loans::repay;
}
