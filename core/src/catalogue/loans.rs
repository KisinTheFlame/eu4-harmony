//! Loans: the player's loans, and repaying one.
use eu4_harmony_remote::{
    loans::{LoanAnswer, LoanRequest, PlayerLoan},
    Error,
};

layer! {
    fn loans() -> Vec<PlayerLoan> = crate::game::loans::list;
    fn repay_loan(loan: u32) -> () = crate::game::loans::repay;
}

pub fn run(request: LoanRequest, game: &mut (impl Layer + ?Sized)) -> Result<LoanAnswer, Error> {
    Ok(match request {
        LoanRequest::List {} => LoanAnswer::List(game.loans()?),
        LoanRequest::Repay { loan } => LoanAnswer::Repay(game.repay_loan(loan)?),
    })
}
