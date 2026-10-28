use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpeningStatus {
    pub name: String,
    pub elapsed: Duration,
}

#[derive(Debug, Default)]
pub struct StartupRuntime {
    pub opening: Option<OpeningStatus>,
    pub cancel_open: bool,
}
