//! EU4 Harmony core. Loaded by the `version.dll` proxy during process start;
//! `eu4_harmony_bootstrap` runs under the loader lock, so it only checks the
//! executable and installs every dependency point, all or none
//! (ADR framework-succeeds-or-fails-as-a-whole). Everything that needs the
//! game's own setup (its file system, the user directory) waits for the game's
//! first `PHYSFS_init`, on the game thread. When startup fails nothing is
//! hooked; a thread of its own writes the report and tells the player.
// The capability groups' list declares every group's modules inside its
// macro, so a module's own macros, such as a test's large `json!`, expand
// deeper with each group the list holds.
#![recursion_limit = "256"]

#[macro_use]
pub(crate) mod base;
#[macro_use]
pub(crate) mod native;
pub(crate) mod text;
pub(crate) mod render;
pub(crate) mod repairs;
pub(crate) mod speed;
pub(crate) mod tweaks;
pub(crate) mod session;
pub(crate) mod interface;
pub(crate) mod world;
pub(crate) mod capabilities;
pub(crate) mod lua;
pub(crate) mod remote;
pub(crate) mod framework_ui;
pub(crate) mod dev;
#[cfg(test)]
pub(crate) mod fake_game;
mod proxy;
pub mod safe_point;
mod speed_registry;
mod startup;
mod startup_failure;
mod this_build;

use base::{heap, icu, lang, log, notice, paths, report, settings};
use native::{baseline, memory, points};
use std::sync::{atomic::Ordering, OnceLock};
use windows_sys::Win32::System::{
    LibraryLoader::GetModuleHandleW,
    Threading::{GetCurrentProcess, TerminateProcess},
};

/// What startup installed, for the report.
static STARTUP: OnceLock<String> = OnceLock::new();

fn wire_base() {
    dev::install_probes();
    log::set_maintenance(lua::maintain_logs);
    report::set_providers(lua::timing_lines, native::speed_up::check_lines);
    settings::set_listener(|listener, current| match listener {
        settings::Listener::Achievements => tweaks::achievements::follow(current),
        settings::Listener::Sound => tweaks::sound::choose(current),
        settings::Listener::Drawing => render::drawing::set(current.frame_rate_limit),
        settings::Listener::CameraZoom => tweaks::camera_zoom::configure(current.zoom_smoothing, current.zoom_step),
    });
    native::speed_up::set_registry(&speed_registry::ALL);
    native::points::set_declared_provider(this_build::declared);
    native::files::set_provider(|| {
        let mut all = text::unicode::framework_fonts::files();
        all.extend(interface::windows::files());
        all
    });
    native::physfs::set_on_first_init(deferred_init);
    render::present::set_hooks(render::present::PresentHooks {
        is_held: framework_ui::held,
        discard_late: interface::tooltips::discard_late,
        before_present: |renderer, device| {
            base::guarded("font pages", text::unicode::fill::grow_pages);
            base::guarded("framework UI frame", || {
                if let Some(device) = device {
                    unsafe { framework_ui::frame(device) }
                }
            });
            base::guarded("native tooltip over framework UI", || interface::tooltips::draw_late(renderer));
            base::guarded("screenshot", || {
                if let Some(device) = device {
                    capabilities::game::screenshot::presenting(device);
                }
            });
        },
        after_main_present: |device| {
            base::guarded("safe point", safe_point::presented);
            base::guarded("input", || {
                if let Some(device) = device {
                    let (width, height) = unsafe { framework_ui::d3d9::Backend::screen(device) };
                    capabilities::game::input::presenting((width as u32, height as u32));
                }
            });
            base::guarded("text costs", text::unicode::costs::log_when_due);
        },
        before_reset: || {
            interface::tooltips::clear();
            framework_ui::device_reset();
            capabilities::game::screenshot::device_reset();
        },
        d3d9_present: |device| unsafe { framework_ui::d3d9::Backend::present(device).is_ok() },
        d3d9_screen: |device| unsafe { framework_ui::d3d9::Backend::screen(device) },
    });
    render::window_input::set_hooks(render::window_input::InputHooks {
        on_real_input: capabilities::game::input::real,
        is_modal: framework_ui::modal,
        on_wheel: tweaks::camera_zoom::before_sdl_wheel,
    });
    render::drawing::set_hooks(render::drawing::DrawingHooks {
        on_prepared_safe_point: safe_point::presented,
        on_prepared_input: capabilities::game::input::presenting,
        is_holding: framework_ui::holding,
        prepare_scene: interface::window_ordering::prepare_scene,
        cursor_now: capabilities::game::input::cursor_now,
    });
    tweaks::hidden::set_hooks(tweaks::hidden::HiddenHooks {
        on_early_sync: proxy::sync_once,
        on_hold_cursor: capabilities::game::input::hold,
    });
    session::sessions::set_listener(|happening| {
        use session::sessions::Happening;
        match happening {
            Happening::Forgotten(_) => {
                capabilities::game::session_forgotten();
                capabilities::game::signals::session_forgotten();
                capabilities::game::map::reset_session();
            }
            Happening::Began(_) => capabilities::game::input::session_began(),
            Happening::Ended(session, end) => {
                log!("session {session} ending ({end:?})");
                capabilities::game::signal_costs::log_session();
                safe_point::session_ended(end);
                session::saves::clear_session();
                base::log::checkpoint();
            }
            Happening::CampaignChanges => {
                capabilities::game::signals::load_begin();
                capabilities::game::orders::world_replaced();
                remote::campaign_changes();
            }
            Happening::WorldReset(world_ptr) => {
                session::saves::clear_session();
                repairs::world_reset(&native::hooked_memory(), world_ptr);
                dev::world_reset();
            }
            Happening::FirstFrameAfterReset => repairs::first_frame_after_reset(&native::hooked_memory()),
            Happening::ResetWithHistory => {
                capabilities::game::signals::world_reset();
                capabilities::game::orders::world_replaced();
                repairs::after_history(&native::hooked_memory());
            }
            Happening::Restarting => {
                speed::saving::background::wait();
                remote::restarting();
            }
        }
    });
    session::saves::set_hooks(session::saves::SaveHooks {
        collect_save_data: lua::collect_save_data,
        join_parallel_save: speed::saving::parallel::join,
        wait_background_save: speed::saving::background::wait,
    });
    interface::set_hooks(interface::InterfaceHooks {
        mod_id: lua::mod_id,
        in_game: |m| world::context(m).is_ok(),
        modifiers_held: capabilities::game::input::modifiers_held,
        on_main_menu_frame: |frontend, show_corner| {
            safe_point::main_menu_frame(frontend);
            speed::loading_times::log_main_menu();
            if show_corner {
                framework_ui::main_menu_frame();
            }
        },
        framework_tooltip_at: framework_ui::tooltip_at,
        player_letters: |m| world::context(m).ok().map(|ctx| String::from_utf8_lossy(&ctx.player.letters()).into_owned()),
        resolve_shield_country: |m, named| {
            let letters = match named {
                Some(s) => s.to_string(),
                None => match world::context(m).ok() {
                    Some(ctx) => String::from_utf8_lossy(&ctx.player.letters()).into_owned(),
                    None => return Ok(None),
                },
            };
            let session = capabilities::game::WorldSession::open(world::Caller::Remote(world::As::Player)).map_err(|e| e.message)?;
            let bytes: [u8; 3] = letters.as_bytes().try_into().map_err(|_| format!("{letters:?} is not a tag"))?;
            let tag = capabilities::game::countries::named(session.m, &session.world, eu4_harmony_remote::CountryTag(bytes)).map_err(|e| e.message)?;
            Ok(Some(tag.0))
        },
    });
    capabilities::set_hooks(capabilities::CapabilityHooks {
        on_safe_frame: safe_point::game_frame,
        reload_extension: |caller, args| {
            let r = lua::reload(caller, args)?;
            Ok(eu4_harmony_remote::extensions::Reloaded { session: r.session, lost: r.lost })
        },
        ui_hold: framework_ui::hold,
        ui_let_go: framework_ui::let_go,
        ui_still: framework_ui::still,
        #[cfg(feature = "dev")]
        answer_memory: dev::answer_memory,
        #[cfg(feature = "dev")]
        answer_performance: dev::answer_performance,
    });
}

fn start(m: memory::Memory) -> std::result::Result<String, Failure> {
    let baseline = baseline::recognize(&m).map_err(|reason| Failure::mismatch(reason, startup_failure::FailureStage::Baseline))?;
    icu::load()?;
    dev::load(&paths::core_dir()?)?;
    if dev::fail_startup() {
        return Err("fail-startup is on in dev.toml's [test-switches]".to_string().into());
    }
    speed_registry::check_switches()?;
    let installed = points::install(m, &this_build::installed(m)).map_err(|reason| Failure::mismatch(reason, startup_failure::FailureStage::Points))?;
    dev::after_install();
    Ok(format!("baseline {baseline}: {} dependency points match; {} hooks, {} writes", installed.checked, installed.hooks, installed.writes))
}

/// Why the startup failed, for the report, and what the player is told.
struct Failure {
    reason: String,
    /// The notice, when it is not the startup failure notice with the reason.
    notice: Option<notice::Message>,
    /// Startup failed because the game folder's `version.dll` is older than
    /// `proxy::OLDEST`.
    outdated_proxy: bool,
    /// Which binary check failed, for cold-path triage on the failure thread.
    triage: Option<startup_failure::FailureStage>,
}

impl Failure {
    fn mismatch(reason: String, stage: startup_failure::FailureStage) -> Self {
        Self { reason, notice: None, outdated_proxy: false, triage: Some(stage) }
    }
}

impl From<String> for Failure {
    fn from(reason: String) -> Self {
        Self { reason, notice: None, outdated_proxy: false, triage: None }
    }
}

fn bootstrap() -> std::result::Result<(), Failure> {
    base::mark_bootstrap();
    base::log_panics();
    wire_base();
    base::MAIN_THREAD.store(base::thread_id(), Ordering::Relaxed);
    let game_dir = paths::game_dir()?;
    // The log stays buffered until the user directory is known (deferred_init).
    log!("eu4-harmony core {}{} bootstrap in {}", env!("CARGO_PKG_VERSION"), build(), game_dir.display());
    tweaks::hidden::read_switches();
    let found = proxy::interface(&game_dir);
    base::set_game_dir(game_dir);
    log!("proxy interface: {}", found.map_or_else(|| "no proxy in the game folder".to_string(), |i| i.to_string()));
    proxy::check(found).map_err(|reason| Failure {
        reason,
        notice: Some(notice::Message::new("EU4HARMONY_PROXY_OUTDATED")),
        outdated_proxy: true,
        triage: None,
    })?;
    let m = memory::Memory { base: unsafe { GetModuleHandleW(std::ptr::null()) } as usize };
    native::set_memory(m);
    let summary = start(m)?;
    log!("startup: {summary}");
    log!("memory at startup: {}", heap::summary());
    let _ = STARTUP.set(summary);
    Ok(())
}

/// What this core is, after its version: its api, and in a developer build
/// or test-games build what it is.
fn build() -> String {
    let api = format!(" (api {})", eu4_harmony_remote::API_VERSION);
    match dev::build_label() {
        Some(label) => format!("{api}, {label}"),
        None => api,
    }
}

/// The report's first lines: this core, and the game folder when it is known;
/// in a developer build, its tools' options.
fn report_header(game_dir: Option<&std::path::Path>) {
    report::line(format!("Harmony {}{}, started {}", env!("CARGO_PKG_VERSION"), build(), log::now()));
    report::line(format!("run: {}, process {}", eu4_harmony_logging::run_id(), std::process::id()));
    if let Ok(path) = paths::core_file() {
        let metadata = path.metadata().ok();
        let bytes = metadata.as_ref().map_or(0, |m| m.len());
        let modified = metadata.and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_millis());
        let identity = eu4_harmony_logging::redact(&format!("core image on disk: {} (bytes={bytes}, modified_unix_ms={modified})", path.display()));
        log::event(format_args!("{identity}"));
        report::line(identity);
    }
    if let Some(dir) = game_dir {
        report::line(format!("game: {}", dir.display()));
    }
    for line in dev::report_lines() {
        report::line(line);
    }
    for line in native::speed_up::report_lines() {
        report::line(line);
    }
}

/// Startup failed and nothing was hooked: writes the log and report and tells
/// the player, from a thread of its own since the loader lock is held here.
/// A hidden start ends the game instead: without the framework it could be
/// neither used nor hidden.
fn fail_startup(reason: String, mut told: Option<notice::Message>, outdated_proxy: bool, triage: Option<startup_failure::FailureStage>) {
    let game_dir = base::game_dir().map(std::path::Path::to_path_buf);
    std::thread::spawn(move || {
        let user_dir = game_dir.as_deref().and_then(|d| paths::user_dir(d).ok());
        match &user_dir {
            Some(dir) => {
                base::open_logs(dir);
            }
            None => base::open_fallback_log(),
        }
        if let Some(dir) = &user_dir {
            lang::decide(dir, settings::read(dir).language);
        }
        if outdated_proxy {
            if let Some(dir) = game_dir.as_deref() {
                if proxy::sync_and_restart(dir, user_dir.as_deref()) == proxy::Outcome::UpdatedRestartFailed {
                    told = Some(notice::Message::new("EU4HARMONY_PROXY_UPDATED_RESTART"));
                }
            }
        }
        let diagnosis = if told.is_none() { triage.zip(game_dir.as_deref()).and_then(|(stage, dir)| startup_failure::diagnose(dir, stage)) } else { None };
        if let Some(d) = &diagnosis {
            log!("startup diagnosis: {}", d.summary());
            told = Some(d.notice());
        }
        report_header(game_dir.as_deref());
        if let Some(d) = &diagnosis {
            report::line(format!("diagnosis: {}", d.summary()));
        }
        report::line(format!("startup FAILED; nothing was hooked and no extension runs:\n{reason}"));
        report::write();
        if tweaks::hidden::hidden_start() {
            log!("startup failed in a hidden start, ending the game: {}", reason.replace('\n', "; "));
            unsafe { TerminateProcess(GetCurrentProcess(), 3) };
        }
        // A changed game can fail every point; the report keeps the full list.
        const SHOWN: usize = 8;
        let lines: Vec<&str> = reason.lines().collect();
        let detail = match lines.len().checked_sub(SHOWN) {
            Some(more) if more > 0 => format!("{}\n… ({more} more in eu4-harmony/logs/report.txt)", lines[..SHOWN].join("\n")),
            _ => reason.clone(),
        };
        notice::tell_and_wait(&told.unwrap_or_else(|| notice::Message::new("EU4HARMONY_STARTUP_FAILED").arg("DETAIL", detail)));
    });
}

/// Runs once, on the game thread, from the first PhysFS call: the rest of
/// startup (`startup`).
pub(crate) fn deferred_init() {
    startup::run(base::game_dir().map(std::path::Path::to_path_buf).unwrap_or_default(), STARTUP.get().map_or("?", String::as_str));
}

/// Called by the proxy from its DllMain. Returns 0 on success.
#[no_mangle]
pub extern "system" fn eu4_harmony_bootstrap() -> u32 {
    match std::panic::catch_unwind(bootstrap) {
        Ok(Ok(())) => 0,
        Ok(Err(Failure { reason, notice, outdated_proxy, triage })) => {
            log!("startup failed: {reason}");
            fail_startup(reason, notice, outdated_proxy, triage);
            1
        }
        Err(_) => {
            log!("startup failed: panic");
            fail_startup("panic during startup".into(), None, false, None);
            2
        }
    }
}
