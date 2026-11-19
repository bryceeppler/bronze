use crate::board::Scope;

#[derive(Clone, Debug, PartialEq)]
pub struct Prefs {
    pub keep_on_top: bool,
    pub scope: Scope,
    pub frame: Option<Frame>,
    pub dismissed_gestures_notice: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub position: (i32, i32),
    pub size: (f32, f32),
}

impl Default for Prefs {
    fn default() -> Prefs {
        Prefs { keep_on_top: true, scope: Scope::All, frame: None, dismissed_gestures_notice: false }
    }
}
