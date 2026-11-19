#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod board;
mod inline;
mod platform;
mod prefs;
mod store;
mod sync;
mod taps;
mod ui;

use std::path::{Path, PathBuf};
use std::rc::Rc;

use rune::{AppSpec, Display, MacWindowOptions, WindowOptions, WindowSpec};

use board::Board;
use platform::{Fake, Native, Platform};
use prefs::Prefs;
use store::Store;
use sync::Syncing;
use ui::{App, Global, Saved, Window};

pub const APP_ID: &str = "ca.bryce.bronze";

const SIZE: (f32, f32) = (360.0, 640.0);
const MIN_SIZE: (f32, f32) = (300.0, 400.0);
const EDGE_MARGIN: f32 = 16.0;

struct Launch {
    data: Option<PathBuf>,
    selection: Option<String>,
    link: Option<Global>,
}

fn main() {
    rune::set_app_id(APP_ID);
    let headless = rune::is_headless();
    let launch = match Launch::parse(rune::extra_args(), headless) {
        Ok(launch) => launch,
        Err(message) => {
            eprintln!("bronze: {message}");
            std::process::exit(2);
        }
    };
    let data = launch.data.or_else(|| (!headless).then(store::data_dir).flatten());
    let _owned = match data.as_deref().map(|dir| own(dir, headless)) {
        Some(Ok(Owned::Folder(lock))) => Some(lock),
        Some(Ok(Owned::ByTheRunningBronze)) => return,
        Some(Err(message)) => {
            eprintln!("bronze: {message}");
            std::process::exit(1);
        }
        None => None,
    };
    let saved = match data {
        Some(dir) => open(&dir),
        None => Saved {
            board: if headless { Board::seeded(0) } else { Board::default() },
            notes: Store::Memory,
            prefs: Prefs::default(),
            preferences: Store::Memory,
            sync: Syncing::unsaved(),
        },
    };
    let platform: Rc<dyn Platform> =
        if headless { Rc::new(Fake::selecting(launch.selection)) } else { Rc::new(Native) };
    let stores = (saved.notes.clone(), saved.preferences.clone());
    let frame = saved.prefs.frame;
    let options = WindowOptions {
        borderless: true,
        transparent: cfg!(target_os = "macos"),
        always_on_top: saved.prefs.keep_on_top,
        position: frame.map(|f| f.position),
        min_size: Some(MIN_SIZE),
        ..WindowOptions::default()
    };
    let spec = AppSpec {
        min_size: Some(MIN_SIZE),
        scenes: ui::SCENES,
        on_key: Some(ui::on_key),
        poll: Some(|s, clock, tx| s.poll(clock, tx)),
        ..AppSpec::new("Bronze", frame.map_or(SIZE, |f| f.size), || unreachable!("init builds the state"), ui::view)
    };
    let app = rune::App::new(spec)
        .configured_options(options.macos(MacWindowOptions::default().corner_radius(ui::WINDOW_RADIUS)))
        .keep_running()
        .on_focus(|s, focused, _| {
            if !focused {
                s.remember_frame();
            }
        })
        .on_quit(|s, _| s.remember_frame())
        .on_reopen(|s, _, tx| {
            s.show_panel(tx);
            true
        })
        .on_open_url(|s, link, tx| s.open_link(link, tx));
    let handle = app.handle();
    let window = Window::new(handle.clone());
    let created = window.clone();
    let app = app
        .on_window(move |native| created.created(native))
        .init(move || App::new(saved, platform, window).launched_with(launch.link));
    handle.update(|s, _| s.register_shortcuts());
    if !headless {
        let gestures = handle.clone();
        platform::listen(move |gesture| gestures.update(move |s, tx| s.global(Global::of(gesture), tx)));
    }
    rune::run_windows(move |displays| {
        let mut window = WindowSpec::from_app("main", app);
        if frame.is_none() {
            window.options.position = displays.iter().find(|d| d.primary).or(displays.first()).map(beside_right_edge);
        }
        [window]
    });
    stores.0.flush();
    stores.1.flush();
}

fn beside_right_edge(display: &Display) -> (i32, i32) {
    let scale = display.scale as f32;
    let (width, height) = (display.size.0 as f32, display.size.1 as f32);
    let x = width - (SIZE.0 + EDGE_MARGIN) * scale;
    let y = (height - SIZE.1 * scale) / 2.0;
    (display.position.0 + x.round() as i32, display.position.1 + y.round() as i32)
}

enum Owned {
    Folder(std::fs::File),
    ByTheRunningBronze,
}

fn own(dir: &Path, headless: bool) -> Result<Owned, String> {
    let folder = std::fs::create_dir_all(dir)
        .and_then(|()| std::fs::canonicalize(dir))
        .map_err(|error| format!("cannot use {} for notes: {error}", dir.display()))?;
    if !headless && !rune::single_instance(&instance_id(&folder)) {
        return Ok(Owned::ByTheRunningBronze);
    }
    store::claim(&folder).map(Owned::Folder).map_err(|why| format!("cannot use {} for notes: {why}", dir.display()))
}

fn instance_id(folder: &Path) -> String {
    let fnv1a = folder
        .as_os_str()
        .as_encoded_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3));
    format!("{APP_ID}-{fnv1a:016x}")
}

fn open(dir: &Path) -> Saved {
    let (board, notes) = store::open::<Board>(dir);
    let (prefs, preferences) = store::open::<Prefs>(dir);
    Saved { board, notes, prefs, preferences, sync: Syncing::saved_in(dir) }
}

impl Launch {
    fn parse(args: &[String], headless: bool) -> Result<Launch, String> {
        let usage = || {
            format!(
                "unexpected arguments {args:?}; usage: bronze [--data <dir>] [bronze://capture | bronze://show-or-hide] [--selection <text>, headless runs only]"
            )
        };
        let mut launch = Launch { data: None, selection: None, link: None };
        let mut args = args.iter();
        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--data" => launch.data = Some(PathBuf::from(args.next().ok_or_else(usage)?)),
                "--selection" if headless => launch.selection = Some(args.next().ok_or_else(usage)?.clone()),
                link => launch.link = Some(Global::from_link(link).ok_or_else(usage)?),
            }
        }
        Ok(launch)
    }
}
