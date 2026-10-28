pub mod bsp;
pub mod compare;
pub mod logic;
pub mod nav;
pub mod nav_gen;
pub mod particles;
mod tr;

pub use tr::TR;

pub fn register_translations() {
    hp_app::register_translations();
    hp_app::i18n::register(TR);
}
