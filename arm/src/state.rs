#[derive(Debug)]
pub struct GlobalState {
    pub is_dobot_busy: bool,
    pub enable_auto_grab: bool,
    pub termiate: bool,
}
