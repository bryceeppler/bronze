mod app;
mod chrome;
mod commands;
mod icons;
mod list;
mod look;
mod overlays;
mod scenes;
mod strike;
#[cfg(test)]
mod tests;

use rune::{Cx, El, col};

pub use app::App;
pub use commands::on_key;
pub use scenes::SCENES;

use look::Look;

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let look = Look::new(cx.system_dark(), cx.env().scale);
    let kit = look.kit();
    let mut layer = Vec::new();
    let panel = col()
        .key("panel")
        .w_full()
        .h_full()
        .fill(look.bg)
        .padding(12.0)
        .child(chrome::top_bar(s, &look, &kit, &mut layer))
        .child(list::list(s, cx, &look))
        .child(chrome::composer(s, &look));
    overlays::mount(s, &look, &kit, &mut layer);
    rune_kit::root(panel, layer)
}
