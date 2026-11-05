//! Loans: the player's loans, as the economy view's loan window lists them,
//! and repaying one with its repay button (docs/research/power-spending.md,
//! "Loans").
use super::{context, game, power, thousandths, Caller, GResult, Tag};
use crate::{
    memory::Memory,
    objects::{
        campaign::{country, loan as fields},
        list,
    },
    points::Point,
};
use eu4_harmony_remote::{
    choices::{Amount, Choice, Resource},
    loans::PlayerLoan,
    CountryTag, Date,
};

/// A loan's ID (its type, an i32, then the number, an i32, as every ID of
/// the game's is kept) and whether an estate lent it; the rest of its
/// fields are in [`fields`].
const ID_TYPE: usize = 0x10;
const ID: usize = 0x14;
const ESTATE: usize = 0x36;
/// The interest left to pay on a loan repaid now: `(loan, &out)`, in
/// thousandths of ducats, answering where it wrote it.
const FEE: usize = 0x86b4b0;
/// The most loans believed.
const MOST: usize = 10_000;

pub fn points() -> Vec<Point> {
    vec![
        // The pay_loan command walks the country's loans for the one it
        // names.
        Point::keep(0x5d77beusize, "48 8b 8b e8 0d 00 00 48 8b c1 48 85 c9 0f 84 0e 0d 00 00 4c 39 20 74 11 48 8b 50 10", "loans")
            .uses(&[country::LOANS, list::NEXT]),
        // The loan window's price: the fee, then the amount in thousandths.
        Point::keep(0xeb1d21usize, "48 8d 55 77 48 8b 4b 50 e8 82 97 9b ff 48 8b 4b 50 69 59 2c e8 03 00 00 03 18", "loan's price").uses(&[fields::AMOUNT]),
        Point::keep(FEE, "48 89 5c 24 18 55 56 57 41 56 41 57 48 83 ec 20 48 8b 05 b1 49 ad 01 4c", "loan's fee"),
        // The bank loans are counted by the lender's valid byte.
        Point::keep(0x33e0b0usize, "48 8b 08 8d 53 01 80 79 27 00", "bank loans").uses(&[fields::LENDER + 7]),
    ]
}

/// The player's loans, in the list's order: each loan's address.
fn loans_of(m: &Memory, country: usize) -> GResult<Vec<usize>> {
    let mut out = Vec::new();
    let mut node = m.ptr(country + country::LOANS + list::HEAD)?;
    while node != 0 {
        if out.len() >= MOST {
            return Err("too many loans".to_string().into());
        }
        if m.u8(node + list::REMOVED)? == 0 {
            out.push(m.ptr(node + list::ITEM)?);
        }
        node = m.ptr(node + list::NEXT)?;
    }
    Ok(out)
}

/// What repaying the loan at `loan` costs now, in thousandths of ducats: its
/// amount and the interest left.
fn price(m: &Memory, loan: usize) -> GResult<i64> {
    let mut fee = 0i32;
    let at = m.call(m.at(FEE), &[loan, &mut fee as *mut i32 as usize])?;
    let fee = if at == 0 { fee } else { m.i32(at)? };
    Ok(m.i32(loan + fields::AMOUNT)? as i64 * 1000 + fee as i64)
}

fn read(m: &Memory, country: usize, loan: usize) -> GResult<PlayerLoan> {
    let lender = Tag(m.u64(loan + fields::LENDER)?);
    let price = price(m, loan)?;
    let cost = vec![Amount::new(Resource::Ducats, price as f64 / 1000.0)];
    let repay = if m.i32(country + country::TREASURY)? as i64 >= price {
        Choice::can(cost)
    } else {
        let values = [(c"VAL", format!("{:.2}", price as f64 / 1000.0))];
        Choice::cannot(power::tooltip(m, c"CV_ECONOMY_LOAN_REPAY_CANT_AFFORD_DESC", &values)?, cost)
    };
    Ok(PlayerLoan {
        loan: m.u32(loan + ID)?,
        lender: lender.normal().then(|| CountryTag(lender.letters())),
        estate: m.u8(loan + ESTATE)? != 0,
        amount: m.i32(loan + fields::AMOUNT)? as f64,
        interest: thousandths(m.i32(loan + fields::INTEREST)?),
        expires: Date(m.i32(loan + fields::DUE)?),
        repay,
    })
}

/// The player's loans.
pub fn list(_: Caller) -> GResult<Vec<PlayerLoan>> {
    let m = game()?;
    let ctx = context(m)?;
    loans_of(m, ctx.country)?.into_iter().map(|loan| read(m, ctx.country, loan)).collect()
}

/// The pay_loan command in its written form: the country by its tag and the
/// loan by its ID, as the repay button makes it.
fn written(country: [u8; 3], id: u32, kind: u32) -> String {
    format!("pay_loan={{ country=\"{}\" loan={{ id={id} type={kind} }} }}", String::from_utf8_lossy(&country))
}

/// Repays the player's loan with the ID `id`, as its repay button does once
/// the player confirms.
pub fn repay(_: Caller, id: u32) -> GResult<()> {
    let m = game()?;
    let ctx = context(m)?;
    let Some(loan) = find(m, ctx.country, id)? else {
        return Err(eu4_harmony_checks::loans::not_found(id).into());
    };
    let choice = read(m, ctx.country, loan)?.repay;
    if !choice.can {
        return Err(eu4_harmony_checks::choices::not_allowed("repay the loan", &choice.reasons).into());
    }
    power::post(&written(ctx.player.letters(), id, m.u32(loan + ID_TYPE)?), "repay the loan")
}

fn find(m: &Memory, country: usize, id: u32) -> GResult<Option<usize>> {
    for loan in loans_of(m, country)? {
        if m.u32(loan + ID)? == id {
            return Ok(Some(loan));
        }
    }
    Ok(None)
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_command_names_the_country_and_the_loan() {
        assert_eq!(super::written(*b"FRA", 12, 4713), r#"pay_loan={ country="FRA" loan={ id=12 type=4713 } }"#);
    }
}
