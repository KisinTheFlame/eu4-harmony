//! Loans: the player's loans, as the economy view's loan window lists them,
//! and repaying one with its repay button (docs/research/power-spending.md,
//! "Loans").
use super::{action, power, thousandths, Fail, GResult, Session, Status, Tag};
use crate::{
    memory::Memory,
    objects::{
        campaign::{country, loan as fields},
        list,
    },
    points::Point,
};
use eu4_harmony_remote::loans::args;
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
pub fn list(Session { m, ctx, .. }: Session) -> GResult<Vec<PlayerLoan>> {
    loans_of(m, ctx.country)?.into_iter().map(|loan| read(m, ctx.country, loan)).collect()
}

/// The pay_loan command in its written form: the country by its tag and the
/// loan by its ID, as the repay button makes it.
fn written(country: [u8; 3], id: u32, kind: u32) -> String {
    format!("pay_loan={{ country=\"{}\" loan={{ id={id} type={kind} }} }}", String::from_utf8_lossy(&country))
}

/// Repays the player's loan with the ID `id`, as its repay button does once
/// the player confirms.
pub fn repay(Session { m, ctx, commands, .. }: Session, args::Repay { loan: id }: args::Repay) -> GResult<()> {
    let Some(loan) = find(m, ctx.country, id)? else {
        return Err(not_found(id));
    };
    let choice = read(m, ctx.country, loan)?.repay;
    action::carry_out(commands, &choice, "repay the loan", || Ok(written(ctx.player.letters(), id, m.u32(loan + ID_TYPE)?)))
}

fn find(m: &Memory, country: usize, id: u32) -> GResult<Option<usize>> {
    for loan in loans_of(m, country)? {
        if m.u32(loan + ID)? == id {
            return Ok(Some(loan));
        }
    }
    Ok(None)
}

/// The refusal of a loan ID that is none of the player's loans.
fn not_found(loan: u32) -> Fail {
    Fail::new(Status::NotFound, format!("the player has no loan {loan}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::{
        buffers::{self, Image, Object},
        commands::{record, Refused},
        Caller, Fail, Status,
    };

    #[test]
    fn the_command_names_the_country_and_the_loan() {
        assert_eq!(super::written(*b"FRA", 12, 4713), r#"pay_loan={ country="FRA" loan={ id=12 type=4713 } }"#);
    }

    /// The fee's function: no interest left to pay, written to `out`.
    extern "C" fn no_fee(_: usize, out: usize) -> usize {
        unsafe { *(out as *mut i32) = 0 };
        0
    }

    /// France, with 500 ducats and one loan of 200, its ID 12 of type 4713,
    /// and the session of it whose commands are recorded.
    struct France {
        _image: Image,
        _objects: Vec<Object>,
        session: Session,
    }

    fn france() -> France {
        let image = Image::new();
        image.function(FEE, no_fee as *const ());
        let fra = Tag(u64::from_le_bytes(*b"FRA\0\0\0\0\0"));
        let loan = Object::new(fields::DUE + 4).u32(ID_TYPE, 4713).u32(ID, 12).i32(fields::AMOUNT, 200);
        let node = Object::new(list::REMOVED + 8).points_to(list::ITEM, loan);
        let country = Object::country(fra, country::LOANS + 8).i32(country::TREASURY, 500_000).points_to(country::LOANS + list::HEAD, node);
        let countries = Object::countries(&[&country]);
        let session = buffers::session(image.memory(), 0, fra, country.at(), countries.at(), Caller::Remote);
        France { _image: image, _objects: vec![country, countries], session }
    }

    #[test]
    fn repaying_a_loan_posts_the_repay_button_s_command_through_the_session() {
        let france = france();
        repay(france.session, args::Repay { loan: 12 }).unwrap();
        assert_eq!(record::posted(), [r#"pay_loan={ country="FRA" loan={ id=12 type=4713 } }"#]);
        let Err(Fail { status, message }) = repay(france.session, args::Repay { loan: 13 }) else { panic!("loan 13 was repaid") };
        assert_eq!((status, message.as_str()), (Status::NotFound, "the player has no loan 13"));
        // The command's own check is the game's, and gives no reason.
        record::refuse_next(Refused::Check);
        let Err(Fail { status, message }) = repay(france.session, args::Repay { loan: 12 }) else { panic!("the refused command was posted") };
        assert_eq!((status, message.as_str()), (Status::Rejected, "the game's command refuses to repay the loan, and gives no reason"));
        assert!(record::posted().is_empty());
    }
}
