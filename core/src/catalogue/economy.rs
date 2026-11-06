//! The economy: what its buttons offer, taking a loan, reducing inflation,
//! raising mercantilism.
use eu4_harmony_remote::economy::{EconomyAnswer, EconomyOption, EconomyRequest};

layer! {
    run EconomyRequest => EconomyAnswer;
    Options => fn economy_options() -> Vec<EconomyOption> = session crate::game::economy::options;
    TakeLoan => fn take_loan() -> () = session crate::game::economy::take_bank_loan;
    ReduceInflation => fn reduce_inflation() -> () = session crate::game::economy::reduce;
    RaiseMercantilism => fn raise_mercantilism() -> () = session crate::game::economy::promote;
}
