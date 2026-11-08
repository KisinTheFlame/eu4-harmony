//! The economy: what its buttons offer, taking a loan, reducing inflation,
//! raising mercantilism.
use eu4_harmony_remote::economy::{EconomyAnswer, EconomyOption, EconomyRequest};
use eu4_harmony_remote::economy::args;

layer! {
    run EconomyRequest => EconomyAnswer;
    Options => fn economy_options -> Vec<EconomyOption> = crate::game::economy::options;
    TakeLoan => fn take_loan -> () = crate::game::economy::take_bank_loan;
    ReduceInflation => fn reduce_inflation -> () = crate::game::economy::reduce;
    RaiseMercantilism => fn raise_mercantilism -> () = crate::game::economy::promote;
}
