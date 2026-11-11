//! Sound output (rodio). One sound at a time; failures are logged, never fatal.

pub fn start(sound: &str) {
    log::debug!("sound start {sound}");
}

pub fn stop() {
    log::debug!("sound stop");
}
