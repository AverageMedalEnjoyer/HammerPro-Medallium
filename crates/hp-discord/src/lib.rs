pub mod activity;
pub mod frame;
pub mod session;
pub mod transport;

pub use activity::{Activity, Button, Privacy, Status, Texts, build_activity, game_image_key};
pub use frame::{Frame, FrameError, Opcode};
pub use session::{Options, Presence, Session, Wanted};
pub use transport::{Conn, Connector, SystemConnector};

pub const DEFAULT_APPLICATION_ID: &str = "";

pub fn application_id(setting: &str) -> Option<&str> {
    let id = setting.trim();
    let id = if id.is_empty() { DEFAULT_APPLICATION_ID } else { id };
    (id.len() >= 17 && id.len() <= 20 && id.bytes().all(|b| b.is_ascii_digit())).then_some(id)
}

#[cfg(test)]
mod tests {
    #[test]
    fn application_ids() {
        assert_eq!(super::application_id(" 1234567890123456789 "), Some("1234567890123456789"));
        assert_eq!(super::application_id(""), None);
        assert_eq!(super::application_id("hammerpro"), None);
        assert_eq!(super::application_id("12345"), None);
    }
}
