//! Who the player is (GLOSSARY: Observe mode, Country in view): the country
//! they play, or in observe mode none, with at most one country in view. The
//! world keeps the player's own tag and the country in view side by side;
//! the game's own "the player's country" (`0x811b20`, a dependency point that
//! uses both) is the country in view while the player observes and one is
//! in view, and the player's own tag otherwise.
//!
//! A country is put in view, and taken out, through the game's own setter
//! (`0x763a20`), as the player's Ctrl and click on the map, a country in a
//! panel, a player in the outliner and the menubar's button do: it writes
//! the country in view and, in observe mode, refreshes the fog of war and
//! the map.
use super::{fail, GResult, Status, Tag, View, World};
use crate::{
    memory::Memory,
    objects::campaign::world,
    points::{GameFunction, Point},
};

/// The game's setter of the country in view: (world, tag), the tag by value.
const SET_IN_VIEW: GameFunction<2> = GameFunction::at(0x763a20);
/// The menubar's button taking the view out, which passes `---` with no
/// valid byte.
const LEAVE_VIEW_CALL: usize = 0x817036;
/// The tag the menubar's button passes to take the view out.
const NO_COUNTRY: u64 = 0x2d2d2d;

/// The player of a game going on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Player {
    /// The player plays this country.
    Plays(Tag),
    /// Observe mode: the player plays no country. `own` is the observer's
    /// tag, `in_view` the country in view, if any.
    Observes { own: Tag, in_view: Option<Tag> },
}

impl Player {
    /// The player's own tag. A session lasts while it does: a change of the
    /// country in view leaves it as it is (ADR
    /// observe-mode-acts-as-the-country-in-view).
    pub fn own(self) -> Tag {
        match self {
            Self::Plays(country) => country,
            Self::Observes { own, .. } => own,
        }
    }

    /// The country remote control and extensions act as: the one played, or
    /// the one in view; none while the player observes with none in view.
    pub fn acting(self) -> Option<Tag> {
        match self {
            Self::Plays(country) => Some(country),
            Self::Observes { in_view, .. } => in_view,
        }
    }

    pub fn observing(self) -> bool {
        matches!(self, Self::Observes { .. })
    }

    /// The tag the game takes for the player's country, as its getter
    /// (`0x811b20`) gives it: the fog of war is that country's, and with
    /// none, the observer's, which hides nothing.
    pub fn seen_as(self) -> Tag {
        self.acting().unwrap_or(self.own())
    }
}

/// What a read shows, by the view it reads and the player (GLOSSARY:
/// Player's view; ADR remote-control-reads-the-players-view): the one place
/// that decides it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Eyes {
    /// The full view: everything the game keeps.
    All,
    /// The player's view with a country to act as: the map as its fog of
    /// war and discoveries leave it, and what the panels show only to a
    /// country of its own.
    Of(Tag),
    /// The player's view in observe mode with no country in view: the whole
    /// map, and of no country what the panels show only to itself.
    Map,
}

impl Eyes {
    /// Of a read that acts as `country`.
    pub fn acting(view: View, country: Tag) -> Self {
        Self::of(view, Player::Plays(country))
    }

    pub fn of(view: View, player: Player) -> Self {
        match (view, player.acting()) {
            (View::Full, _) => Self::All,
            (View::Player, Some(country)) => Self::Of(country),
            (View::Player, None) => Self::Map,
        }
    }

    /// The country whose discoveries decide which provinces are terra
    /// incognita; none when every province shows.
    pub fn discovering(self) -> Option<Tag> {
        match self {
            Self::Of(country) => Some(country),
            Self::All | Self::Map => None,
        }
    }

    /// Whether it sees what the game shows only to `country` itself, of the
    /// country or of a province it owns: in the full view, or of the country
    /// acted as.
    pub fn sees_all_of(self, country: Tag) -> bool {
        match self {
            Self::All => true,
            Self::Of(own) => own.index() == country.index(),
            Self::Map => false,
        }
    }

    /// Whether it sees what the province view shows only of a country's own
    /// provinces, in a province owned by `owner`: [`Self::sees_all_of`] its
    /// owner, and in one no country owns.
    pub fn sees_inside(self, owner: Option<Tag>) -> bool {
        owner.is_none_or(|owner| self.sees_all_of(owner))
    }
}

/// Puts `country` in view, as the player's Ctrl and click on one of its
/// provinces does: [`Status::Rejected`] when the player plays a country,
/// not observe mode, or `country` owns no province.
pub fn put_in_view(m: &Memory, world: &World, country: Tag) -> GResult<()> {
    observing(world)?;
    if !country.normal() || super::world::owned_provinces(m, world.world, country)? == 0 {
        return fail(Status::Rejected, format!("{} owns no province to put in view", super::named(country)));
    }
    SET_IN_VIEW.call(m, [world.world, country.0 as usize])?;
    Ok(())
}

/// Takes the country in view out, as the menubar's button does; nothing with
/// none in view. [`Status::Rejected`] when the player plays a country.
pub fn leave_view(m: &Memory, world: &World) -> GResult<()> {
    observing(world)?;
    if world.player.acting().is_some() {
        SET_IN_VIEW.call(m, [world.world, NO_COUNTRY as usize])?;
    }
    Ok(())
}

fn observing(world: &World) -> GResult<()> {
    match world.player {
        Player::Observes { .. } => Ok(()),
        Player::Plays(country) => fail(Status::Rejected, format!("the player plays {}, not observe mode", super::named(country))),
    }
}

pub fn points() -> Vec<Point> {
    vec![
        Point::keep(SET_IN_VIEW, "40 53 48 83 ec 20 48 89 91 68 1e 00 00 48 8b d9 48 8b 89 00 1e 00 00", "set the country in view")
            .uses(&[world::IN_VIEW, world::MAP_MANAGER]),
        Point::keep(LEAVE_VIEW_CALL, "ba 2d 2d 2d 00 49 8b c8 e8 dd c9 f4 ff", "the menubar taking the view out").uses(&[NO_COUNTRY as usize]),
    ]
}

/// The player of the game whose world is `world`.
pub fn read(m: &Memory, world: usize) -> GResult<Player> {
    of(Tag(m.u64(world + world::PLAYER)?), Tag(m.u64(world + world::IN_VIEW)?))
}

/// The player, from the world's two tags: the player's own and the country
/// in view.
pub fn of(own: Tag, in_view: Tag) -> GResult<Player> {
    if own.kind() == world::OBSERVE {
        return Ok(Player::Observes { own, in_view: Some(in_view).filter(|tag| tag.normal()) });
    }
    if !own.normal() {
        return fail(Status::NotInGame, "no player");
    }
    Ok(Player::Plays(own))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::campaign::tag;

    fn tag_of(letters: &[u8; 3], index: i16, kind: u8, valid: bool) -> Tag {
        let mut bytes = [0u8; 8];
        bytes[..3].copy_from_slice(letters);
        let raw = u64::from_le_bytes(bytes) | (index as u16 as u64) << tag::INDEX | (kind as u64) << tag::KIND | (valid as u64) << tag::VALID;
        Tag(raw)
    }

    #[test]
    fn a_country_played_is_acted_as() {
        let fra = tag_of(b"FRA", 42, 0, true);
        let player = of(fra, Tag(0)).unwrap();
        assert_eq!(player, Player::Plays(fra));
        assert_eq!((player.own(), player.acting(), player.observing(), player.seen_as()), (fra, Some(fra), false, fra));
    }

    #[test]
    fn a_country_in_view_is_acted_as_while_the_observer_stays_the_player() {
        let (observer, fra) = (tag_of(b"O00", 900, world::OBSERVE, true), tag_of(b"FRA", 42, 0, true));
        let player = of(observer, fra).unwrap();
        assert_eq!(player, Player::Observes { own: observer, in_view: Some(fra) });
        assert_eq!((player.own(), player.acting(), player.observing(), player.seen_as()), (observer, Some(fra), true, fra));
    }

    #[test]
    fn with_no_country_in_view_none_is_acted_as() {
        let observer = tag_of(b"O00", 900, world::OBSERVE, true);
        // The game takes the view out by writing "---", whose last byte is
        // clear, as it is when nothing was put in view.
        for none in [Tag(0), tag_of(b"---", 0, 0, false), tag_of(b"FRA", 42, 0, false)] {
            let player = of(observer, none).unwrap();
            assert_eq!(player, Player::Observes { own: observer, in_view: None });
            assert_eq!((player.acting(), player.seen_as()), (None, observer));
        }
    }

    #[test]
    fn a_country_in_view_counts_only_in_observe_mode() {
        let (fra, eng) = (tag_of(b"FRA", 42, 0, true), tag_of(b"ENG", 7, 0, true));
        assert_eq!(of(fra, eng).unwrap(), Player::Plays(fra));
    }

    #[test]
    fn the_full_view_sees_all_and_the_player_s_view_the_country_acted_as_or_the_map() {
        let (observer, fra, eng) = (tag_of(b"O00", 900, world::OBSERVE, true), tag_of(b"FRA", 42, 0, true), tag_of(b"ENG", 7, 0, true));
        let (plays, in_view, none) =
            (Player::Plays(fra), Player::Observes { own: observer, in_view: Some(fra) }, Player::Observes { own: observer, in_view: None });
        for player in [plays, in_view, none] {
            assert_eq!(Eyes::of(View::Full, player), Eyes::All);
        }
        assert_eq!(Eyes::of(View::Player, plays), Eyes::Of(fra));
        assert_eq!(Eyes::of(View::Player, in_view), Eyes::Of(fra));
        assert_eq!(Eyes::of(View::Player, none), Eyes::Map);

        assert_eq!((Eyes::All.discovering(), Eyes::Of(fra).discovering(), Eyes::Map.discovering()), (None, Some(fra), None));
        assert!(Eyes::All.sees_all_of(eng) && Eyes::Of(fra).sees_all_of(fra));
        assert!(!Eyes::Of(fra).sees_all_of(eng) && !Eyes::Map.sees_all_of(fra));
        assert!(Eyes::Map.sees_inside(None) && Eyes::Of(fra).sees_inside(Some(fra)));
        assert!(!Eyes::Map.sees_inside(Some(fra)) && !Eyes::Of(fra).sees_inside(Some(eng)));
    }

    #[test]
    fn a_world_with_no_player_is_no_game() {
        assert_eq!(of(Tag(0), Tag(0)).unwrap_err().status, Status::NotInGame);
    }
}
