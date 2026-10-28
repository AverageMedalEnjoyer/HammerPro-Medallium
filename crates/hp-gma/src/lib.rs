pub mod addon_json;
mod archive;
mod error;
mod format;
mod index;
mod legacy;
pub mod whitelist;
mod writer;

pub use addon_json::{AddonJson, AddonProblem};
pub use archive::{Entries, GmaArchive, GmaEntry};
pub use error::GmaError;
pub use index::{Paths, normalize_path};
pub use writer::{GmaSource, GmaSummary, GmaWriter, RejectReason, check_name};
