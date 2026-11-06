//! The economy: what its buttons offer, taking a loan, reducing inflation,
//! raising mercantilism.
use eu4_harmony_remote::economy::{EconomyAnswer, EconomyOption, EconomyRequest};

layer! {
    run EconomyRequest => EconomyAnswer;
    Options => fn economy_options() -> Vec<EconomyOption> = caller crate::game::economy::options;
    TakeLoan => fn take_loan() -> () = caller crate::game::economy::take_bank_loan;
    ReduceInflation => fn reduce_inflation() -> () = caller crate::game::economy::reduce;
    RaiseMercantilism => fn raise_mercantilism() -> () = caller crate::game::economy::promote;
}
