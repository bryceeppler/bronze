mod board;
mod inline;
mod store;
mod ui;

use std::cell::OnceCell;
use std::path::PathBuf;
use std::rc::Rc;

use rune::AppSpec;

use board::Board;
use store::Store;
use ui::App;

fn main() {
    let data = match data_dir_arg(rune::extra_args()) {
        Ok(data) => data,
        Err(message) => {
            eprintln!("bronze: {message}");
            std::process::exit(2);
        }
    };
    let opened: Rc<OnceCell<Store<Board>>> = Rc::default();
    let keep = opened.clone();
    let spec = AppSpec {
        min_size: Some((300.0, 400.0)),
        scenes: ui::SCENES,
        on_key: Some(ui::on_key),
        ..AppSpec::new("Bronze", (360.0, 640.0), || App::new(Board::default(), Store::Memory), ui::view)
    };
    rune::App::new(spec)
        .init(move || {
            let dir = data.or_else(|| rune::tooling::headless_run().is_none().then(store::data_dir).flatten());
            let (board, store) = match dir {
                Some(dir) => store::open(&dir),
                None => (Board::default(), Store::Memory),
            };
            let _ = keep.set(store.clone());
            App::new(board, store)
        })
        .run();
    if let Some(store) = opened.get() {
        store.flush();
    }
}

fn data_dir_arg(args: &[String]) -> Result<Option<PathBuf>, String> {
    match args {
        [] => Ok(None),
        [flag, dir] if flag == "--data" => Ok(Some(PathBuf::from(dir))),
        _ => Err(format!("unexpected arguments {args:?}; usage: bronze [--data <dir>]")),
    }
}
