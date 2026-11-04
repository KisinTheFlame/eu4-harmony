//! The economy: what its buttons offer, taking a loan, reducing inflation,
//! raising mercantilism.
use eu4_harmony_remote::{
    economy::{EconomyAnswer, EconomyOption, EconomyRequest},
    Error,
};

layer! {
    fn economy_options() -> Vec<EconomyOption> = crate::game::economy::options;
    fn take_loan() -> () = crate::game::economy::take_bank_loan;
    fn reduce_inflation() -> () = crate::game::economy::reduce;
    fn raise_mercantilism() -> () = crate::game::economy::promote;
}

pub fn run(request: EconomyRequest, game: &mut (impl Layer + ?Sized)) -> Result<EconomyAnswer, Error> {
    Ok(match request {
        EconomyRequest::Options {} => EconomyAnswer::Options(game.economy_options()?),
        EconomyRequest::TakeLoan {} => EconomyAnswer::TakeLoan(game.take_loan()?),
        EconomyRequest::ReduceInflation {} => EconomyAnswer::ReduceInflation(game.reduce_inflation()?),
        EconomyRequest::RaiseMercantilism {} => EconomyAnswer::RaiseMercantilism(game.raise_mercantilism()?),
    })
}
