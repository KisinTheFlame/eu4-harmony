//! Tries remote control in the running game before any transport exists: with
//! `EU4HARMONY_TEST_REMOTE` set when `eu4.exe` starts, a thread of its own
//! submits requests every few seconds and writes what comes back to the
//! framework log, in the main menu and in a game. Set to `orders`, it also
//! moves one of the player's land armies to a neighboring province and stops
//! it, once a session, while the game runs.
use super::submit;
use eu4_harmony_remote::{
    armies::{ArmyAnswer, ArmyRequest},
    countries::CountryRequest,
    describe::{DescribeAnswer, DescribeRequest},
    events::{EventsAnswer, EventsRequest},
    provinces::{ProvinceAnswer, ProvinceRequest},
    session::{SessionAnswer, SessionRequest},
    texts::TextRequest,
    Answer, CountryTag, Request,
};
use std::time::Duration;

/// Starts the probe when `EU4HARMONY_TEST_REMOTE` is set.
pub fn start() {
    let Some(mode) = std::env::var_os("EU4HARMONY_TEST_REMOTE") else { return };
    let orders = mode == "orders";
    log!("remote probe: started (orders: {orders})");
    std::thread::spawn(move || run(orders));
}

/// Submits a request and waits for its answer, logging it.
fn ask(request: Request) -> Option<Answer> {
    let name = request.name();
    match submit(request).wait(Duration::from_secs(30)) {
        Some(Ok(answer)) => {
            let mut text = serde_json::to_string(&answer).unwrap_or_default();
            if text.len() > 300 {
                text = format!("{}... ({} bytes)", &text[..text.floor_char_boundary(300)], text.len());
            }
            log!("remote probe: {name} -> {text}");
            Some(answer)
        }
        Some(Err(e)) => {
            log!("remote probe: {name} -> {e}");
            None
        }
        None => {
            log!("remote probe: {name} -> no answer in 30 s");
            None
        }
    }
}

fn run(orders: bool) {
    let mut since = 0;
    let mut ordered_in = None;
    loop {
        std::thread::sleep(Duration::from_secs(5));
        if let Some(Answer::Describe(DescribeAnswer::All(all))) = ask(Request::Describe(DescribeRequest::All {})) {
            log!("remote probe: {} capabilities", all.len());
        }
        ask(Request::Text(TextRequest::Language {}));
        if let Some(Answer::Events(EventsAnswer::Read(read))) = ask(Request::Events(EventsRequest::Read { since: Some(since), wait_ms: Some(2000) })) {
            since = read.last;
        }
        let Some(Answer::Session(SessionAnswer::Get(session))) = ask(Request::Session(SessionRequest::Get {})) else { continue };
        ask(Request::Country(CountryRequest::Hostile { country: session.player, other: CountryTag(*b"BYZ") }));
        ask(Request::Country(CountryRequest::Hostile { country: session.player, other: CountryTag(*b"ZZZ") }));
        if let Some(Answer::Province(ProvinceAnswer::List(all))) = ask(Request::Province(ProvinceRequest::List {})) {
            let owned = all.iter().filter(|p| p.owner == Some(session.player)).count();
            log!("remote probe: {} provinces, {owned} of them the player's", all.len());
        }
        ask(Request::Army(ArmyRequest::Selected {}));
        let Some(Answer::Army(ArmyAnswer::List(armies))) = ask(Request::Army(ArmyRequest::List {})) else { continue };
        let Some(army) = armies.iter().find(|a| a.land && a.route_len == 0) else { continue };
        let Some(at) = army.province else { continue };
        ask(Request::Army(ArmyRequest::Get { army: army.id }));
        ask(Request::Army(ArmyRequest::InProvince { province: at }));
        let Some(Answer::Province(ProvinceAnswer::Neighbors(neighbors))) = ask(Request::Province(ProvinceRequest::Neighbors { province: at })) else {
            continue;
        };
        let Some(next) = neighbors.iter().find(|n| !n.strait) else { continue };
        ask(Request::Army(ArmyRequest::CanEnter { army: army.id, province: next.province }));
        if let Some(strait) = neighbors.iter().find(|n| n.strait) {
            ask(Request::Army(ArmyRequest::StraitOpen { army: army.id, from: at, to: strait.province }));
        }
        if !orders || session.paused || ordered_in == Some(session.session) {
            continue;
        }
        ordered_in = Some(session.session);
        ask(Request::Army(ArmyRequest::FindRoute { army: army.id, goal: next.province }));
        ask(Request::Army(ArmyRequest::Move { army: army.id, goal: next.province }));
        ask(Request::Army(ArmyRequest::Route { army: army.id }));
        ask(Request::Army(ArmyRequest::Get { army: army.id }));
        ask(Request::Army(ArmyRequest::Stop { army: army.id }));
        ask(Request::Army(ArmyRequest::Route { army: army.id }));
    }
}
