//! `font-growth`: verifies that Unicode font page growth and map-name rebuilds
//! actually execute while in-game with `drawing-thread` active, using
//! `[test-switches] grow-pages = true` on the developer build.
//!
//! Catches regressions where `text::unicode::grow_pages` is only wired to
//! `before_main_present` (which is skipped while `drawing-thread` presents
//! in-game frames), leaving newly rasterized CJK glyphs stuck as `?`.
use super::{logs::Mark, text, Build, Game, Parameter, Parameters, Played, Scenario};
use eu4_harmony_remote::{menu::MenuRequest, session::Session, CountryTag};
use serde_json::{json, Value};

pub struct FontGrowth;

impl Scenario for FontGrowth {
    fn name(&self) -> &'static str {
        "font-growth"
    }

    fn about(&self) -> &'static str {
        "in-game Unicode font page growth via test-switches while drawing-thread presents frames"
    }

    fn build(&self, _: &Parameters) -> Build {
        Build::Dev
    }

    fn options(&self, _: &Parameters) -> Option<String> {
        Some("[test-switches]\ngrow-pages = true\n".into())
    }

    fn minutes(&self) -> u32 {
        2
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[]
    }

    fn play(&self, game: &mut Game, _: &Parameters, record: &mut Value) -> Played<()> {
        let user = game.user()?;
        let mark = Mark::of(&user);

        let _: Session = game.ask(&MenuRequest::Start { country: Some(CountryTag(*b"FRA")) })?;
        game.run(5)?;

        // Inspect `core.log` BEFORE returning to the main menu: the regression
        // only deferred `grow_pages` until the main-menu present path ran.
        let in_game_lines = mark.since(&user.core_log());
        let summary = analyze_font_growth_lines(&in_game_lines);
        *record = json!({
            "switch_triggered": summary.switch_triggered,
            "grown_pages": summary.grown_pages,
            "map_names_rebuilt": summary.map_names_rebuilt,
            "growth_failures": summary.growth_failures,
        });

        game.check(summary.switch_triggered, "test switch `grow-pages` did not log its trigger on the first in-game frame")?;
        game.check(summary.grown_pages > 0, "no font pages grew while in-game before returning to the main menu (check drawing-thread grow_pages hooks)")?;
        game.check(summary.map_names_rebuilt, "map names were not rebuilt in-game after map font page growth")?;
        game.check(summary.growth_failures.is_empty(), format!("font page growth failures in-game: {:?}", summary.growth_failures))?;

        game.return_to_menu()
    }

    fn report(&self, record: &Value) -> Vec<String> {
        vec![
            "### In-game font page growth".into(),
            String::new(),
            format!(
                "Switch triggered: {}. Pages grown in-game: {}. Map names rebuilt in-game: {}. Failures: {}.",
                text(&record["switch_triggered"]),
                text(&record["grown_pages"]),
                text(&record["map_names_rebuilt"]),
                text(&record["growth_failures"]),
            ),
        ]
    }
}

#[derive(Debug, Default, PartialEq, Eq)]
struct FontGrowthSummary {
    switch_triggered: bool,
    grown_pages: usize,
    map_names_rebuilt: bool,
    growth_failures: Vec<String>,
}

fn analyze_font_growth_lines(lines: &[String]) -> FontGrowthSummary {
    let mut summary = FontGrowthSummary::default();
    for line in lines {
        if line.contains("test switch grow-pages: every font page grows") {
            summary.switch_triggered = true;
        }
        if line.contains("fill: the page of font ") && line.contains(" grew from ") {
            summary.grown_pages += 1;
        }
        if line.contains("fill: the names on the map built again in ") {
            summary.map_names_rebuilt = true;
        }
        if line.contains("fill: the page of font ") && line.contains(" keeps its size: ") {
            summary.growth_failures.push(line.clone());
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognizes_in_game_font_page_growth_and_failures() {
        let ok = vec![
            "04:00:01 test switch grow-pages: every font page grows".to_string(),
            "04:00:01 fill: the page of font \"vic_18\" grew from 512x512 to 512x1024, for 3 fonts: +1 MB".to_string(),
            "04:00:01 fill: the names on the map built again in 12.3ms".to_string(),
        ];
        assert_eq!(
            analyze_font_growth_lines(&ok),
            FontGrowthSummary { switch_triggered: true, grown_pages: 1, map_names_rebuilt: true, growth_failures: Vec::new() }
        );

        let broken = vec![
            "04:00:01 test switch grow-pages: every font page grows".to_string(),
            "04:00:02 fill: the page of font \"vic_18\" keeps its size: out of video memory".to_string(),
        ];
        let summary = analyze_font_growth_lines(&broken);
        assert!(summary.switch_triggered);
        assert_eq!(summary.grown_pages, 0);
        assert!(!summary.map_names_rebuilt);
        assert_eq!(summary.growth_failures.len(), 1);
    }
}
