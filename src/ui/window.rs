use std::cell::RefCell;
use std::rc::Rc;

use rune::{AppHandle, ShortcutError, Tx, WindowHandle};

use super::app::App;
use super::commands::Global;
use crate::prefs::Frame;

#[derive(Clone)]
pub struct Window {
    app: AppHandle<App>,
    native: Rc<RefCell<Option<WindowHandle>>>,
}

impl Window {
    pub fn new(app: AppHandle<App>) -> Window {
        Window { app, native: Rc::default() }
    }

    pub fn created(&self, native: &WindowHandle) {
        *self.native.borrow_mut() = Some(native.clone());
    }

    pub fn hide(&self) {
        self.app.hide();
    }

    pub fn open(&self) {
        self.app.open();
    }

    pub fn keep_on_top(&self, on: bool) {
        if let Some(native) = &*self.native.borrow() {
            native.set_always_on_top(on);
        }
    }

    pub fn frame(&self) -> Option<Frame> {
        let placement = self.native.borrow().as_ref()?.placement()?;
        let (width, height) = placement.content_size;
        let scale = placement.scale as f32;
        Some(Frame { position: placement.position, size: (width as f32 / scale, height as f32 / scale) })
    }

    pub fn waker(&self) -> impl Fn() + Send + 'static {
        let app = self.app.clone();
        move || app.wake()
    }

    pub fn register(&self, global: Global) -> Result<(), ShortcutError> {
        let shortcut = global.chord().parse()?;
        self.app.register_shortcut(shortcut, move |s: &mut App, tx: &mut Tx| s.global(global, tx)).map(drop)
    }
}
