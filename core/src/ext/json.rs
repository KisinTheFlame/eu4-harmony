//! The snapshots that cross the ABI as JSON (ADR 0018), written as the
//! `abi::GameApi` fields describe them; `abi/json/` holds one of each, which
//! the tests below and the SDK's read.
use crate::game::{countries, diplomacy, offers, provinces, questions, thousandths, Tag};

/// A province's snapshot as the JSON `abi::GameApi::province` describes.
pub(super) fn province_json(s: &provinces::Snapshot) -> serde_json::Value {
    let country = |t: Option<Tag>| t.map(|t| t.0);
    serde_json::json!({
        "id": s.id,
        "name": s.name,
        "owner": country(s.owner),
        "controller": country(s.controller),
        "development": {"tax": thousandths(s.development.tax), "production": thousandths(s.development.production), "manpower": thousandths(s.development.manpower)},
        "terrain": s.terrain,
        "fort": {"level": s.fort.level, "mothballed": s.fort.mothballed},
        "buildings": s.buildings,
        "recruit": s.recruit.iter().map(|r| serde_json::json!({"kind": r.kind.key(), "unit_type": r.unit_type, "allowed": r.allowed, "reasons": r.reasons})).collect::<Vec<_>>(),
    })
}

/// A country's snapshot as the JSON `abi::GameApi::country` describes.
pub(super) fn country_json(c: &countries::Country) -> serde_json::Value {
    let ids = |tags: &[Tag]| tags.iter().map(|t| t.0).collect::<Vec<_>>();
    serde_json::json!({
        "id": c.tag.0,
        "treasury": thousandths(c.treasury),
        "manpower": c.manpower,
        "max_manpower": c.max_manpower,
        "stability": thousandths(c.stability),
        "prestige": thousandths(c.prestige),
        "power": {"adm": c.power[0], "dip": c.power[1], "mil": c.power[2]},
        "wars": c.wars.iter().map(|w| serde_json::json!({"name": w.name, "attackers": ids(&w.attackers), "defenders": ids(&w.defenders)})).collect::<Vec<_>>(),
        "relations": c
            .relations
            .iter()
            .map(|r| {
                let mut relation = serde_json::json!({"kind": r.kind.key(), "country": r.country.0});
                if let Some(key) = &r.subject_type {
                    relation["subject_type"] = key.as_str().into();
                }
                relation
            })
            .collect::<Vec<_>>(),
    })
}

/// The questions as the JSON `abi::GameApi::questions` describes.
pub(super) fn questions_json(list: &[questions::Question]) -> serde_json::Value {
    list.iter()
        .map(|q| {
            let options: Vec<_> = q.options.iter().map(|o| serde_json::json!({"index": o.index, "text": o.text})).collect();
            serde_json::json!({"id": q.id, "event": q.event, "title": q.title, "description": q.description, "deadline": q.deadline, "options": options})
        })
        .collect()
}

/// The offers as the JSON `abi::GameApi::offers` describes.
pub(super) fn offers_json(list: &[offers::Offer]) -> serde_json::Value {
    list.iter().map(|o| serde_json::json!({"id": o.id, "from": o.from.0, "date": o.date, "title": o.title, "description": o.description})).collect()
}

/// The war goals offered, as the JSON `abi::GameApi::war_goals` describes.
pub(super) fn war_goals_json(goals: &[diplomacy::WarGoal]) -> serde_json::Value {
    let goal = |g: &diplomacy::WarGoal| {
        serde_json::json!({
            "casus_belli": g.casus_belli.as_deref().unwrap_or(diplomacy::NO_CASUS_BELLI),
            "war_goal": g.war_goal,
            "province": g.province,
            "target": g.target.map(|t| t.0),
            "name": g.name,
        })
    };
    serde_json::Value::Array(goals.iter().map(goal).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::diplomacy;
    use crate::game::{countries::RelationKind, provinces::UnitKind};

    /// The snapshots the SDK's tests read (ADR 0018).
    const PROVINCE_JSON: &str = include_str!("../../../abi/json/province.json");
    const COUNTRY_JSON: &str = include_str!("../../../abi/json/country.json");
    const WAR_GOALS_JSON: &str = include_str!("../../../abi/json/war_goals.json");

    /// Constantinople as the engine's fake game has it, every kind of unit
    /// in it, is written as the same JSON value as the file holds: the same
    /// members, holding the same values, each number of the same sort
    /// (`10.0`, not `10`). Only the spaces and the order of the members may
    /// differ, which no JSON reader sees.
    #[test]
    fn a_province_snapshot_is_written_as_the_abi_says() {
        let snapshot = crate::remote::fake::constantinople();
        assert_eq!(snapshot.recruit.iter().map(|r| r.kind).collect::<Vec<_>>(), UnitKind::ALL);
        assert_eq!(province_json(&snapshot), serde_json::from_str::<serde_json::Value>(PROVINCE_JSON).unwrap());
    }

    /// Wallachia as the engine's fake game has it, a relation of every kind
    /// and a war on either side, is written as the same JSON value as the
    /// file holds, as a province's is.
    #[test]
    fn a_country_snapshot_is_written_as_the_abi_says() {
        let snapshot = crate::remote::fake::wallachia();
        assert_eq!(snapshot.relations.iter().map(|r| r.kind).collect::<Vec<_>>(), RelationKind::ALL);
        assert_eq!(country_json(&snapshot), serde_json::from_str::<serde_json::Value>(COUNTRY_JSON).unwrap());
    }

    /// The questions and offers the SDK's test reads (ADR 0018).
    const QUESTIONS_JSON: &str = include_str!("../../../abi/json/questions.json");
    const OFFERS_JSON: &str = include_str!("../../../abi/json/offers.json");

    /// A question (the engine's fake one, its title in Chinese) and an offer
    /// are written as the same JSON values as the files hold.
    #[test]
    fn questions_and_offers_are_written_as_the_abi_says() {
        let question = questions::Question { title: "禁卫军".into(), ..crate::remote::fake::question() };
        assert_eq!(questions_json(&[question]), serde_json::from_str::<serde_json::Value>(QUESTIONS_JSON).unwrap());
        let offer =
            offers::Offer { id: 3, from: crate::remote::fake::byz(), date: 0x035D_7710, title: "皇室联姻".into(), description: "A royal marriage.".into() };
        assert_eq!(offers_json(&[offer]), serde_json::from_str::<serde_json::Value>(OFFERS_JSON).unwrap());
    }

    /// War goals with a province, with a country and of none are written as
    /// the same JSON value as the file holds, as a province's snapshot is.
    #[test]
    fn the_war_goals_offered_are_written_as_the_abi_says() {
        let goal = |cb: Option<&str>, wg: Option<&str>, province, target, name: &str| diplomacy::WarGoal {
            casus_belli: cb.map(str::to_string),
            war_goal: wg.map(str::to_string),
            province,
            target,
            name: name.into(),
        };
        let goals = [
            goal(Some("cb_claim"), Some("take_claim"), Some(151), None, "征服君士坦丁堡"),
            goal(
                Some("cb_restore_personal_union"),
                Some("take_capital_personal_union"),
                None,
                Some(crate::remote::fake::byz()),
                "Restore the Union with Byzantium",
            ),
            goal(None, Some("nowargoal"), None, None, "无战争目标"),
        ];
        assert_eq!(war_goals_json(&goals), serde_json::from_str::<serde_json::Value>(WAR_GOALS_JSON).unwrap());
    }
}
