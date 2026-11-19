mod app;
mod chrome;
mod commands;
mod icons;
mod list;
mod look;
mod overlays;
mod scenes;
mod strike;
mod sync_sheet;
#[cfg(test)]
mod tests;
mod window;

use rune::{Cx, El, col};

pub use app::{App, Saved};
pub use commands::{Global, on_key};
pub use scenes::SCENES;
pub use window::Window;

use look::Look;

pub const WINDOW_RADIUS: f32 = if cfg!(target_os = "macos") { 12.0 } else { 0.0 };

pub fn view(s: &App, cx: &mut Cx<App>) -> El {
    let look = Look::new(cx.system_dark(), cx.env().scale);
    let kit = look.kit();
    let mut layer = Vec::new();
    let panel = col()
        .key("panel")
        .w_full()
        .h_full()
        .fill(look.bg)
        .radius(WINDOW_RADIUS)
        .border(look.hair, look.card_border)
        .px(12.0)
        .pb(12.0)
        .child(chrome::top_bar(s, &look, &kit, &mut layer))
        .child(list::list(s, cx, &look))
        .children(chrome::notice(s, &look))
        .child(chrome::composer(s, &look));
    overlays::mount(s, &look, &kit, &mut layer);
    rune_kit::root(panel, layer)
}
