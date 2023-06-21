use once_cell::sync::Lazy;
use tokio::runtime::Runtime;

pub mod audio_player;
pub mod chat_client;
pub mod filter;
pub mod ui_app;

pub const DEFAULT_FONT_SIZE: f32 = 18.0;
pub const MAX_MESSAGE_COUNT: usize = 1000;
pub const APP_SAVE_STATE_KEY: &str = "save";

static ASYNC_RUNTIME: Lazy<Runtime> = Lazy::new(|| {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap()
});
