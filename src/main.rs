#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

use eframe::{
    egui::{FontData, FontDefinitions, TextureOptions},
    epaint::{ColorImage, FontFamily},
    //Theme,
};
use twitch_chat_watcher::{ui_app::*, APP_SAVE_STATE_KEY};

fn main() -> Result<(), eframe::Error> {
    env_logger::init(); // Log to stderr (if you run with `RUST_LOG=debug`).

    let options = eframe::NativeOptions {
        //follow_system_theme: false,
        //default_theme: Theme::Light,
        ..Default::default()
    };
    eframe::run_native(
        "Twitch chat watcher",
        options,
        Box::new(|cc: &eframe::CreationContext<'_>| {
            let mut fonts = FontDefinitions::default();
            //fonts.font_data.insert(
            //    "Iansui-Regular".to_owned(),
            //    FontData::from_static(include_bytes!("../fonts/Iansui-Regular.ttf")),
            //);
            //fonts.font_data.insert(
            //    "NotoTc".to_owned(),
            //    FontData::from_static(include_bytes!("../fonts/NotoSansTC-Regular.otf")),
            //);
            //fonts.font_data.insert(
            //    "NotoSc".to_owned(),
            //    FontData::from_static(include_bytes!("../fonts/NotoSansSC-Regular.otf")),
            //);
            fonts.font_data.insert(
                "NotoMerged".to_owned(),
                FontData::from_static(include_bytes!("../fonts/NotoSansMerged-Regular.otf")),
            );
            fonts
                .families
                .get_mut(&FontFamily::Proportional)
                .unwrap()
                .insert(0, "NotoMerged".to_owned());
            //fonts
            //    .families
            //    .get_mut(&FontFamily::Monospace)
            //    .unwrap()
            //    .push("Iansui-Regular".to_owned());
            cc.egui_ctx.set_fonts(fonts);

            set_font_size(&cc.egui_ctx, twitch_chat_watcher::DEFAULT_FONT_SIZE);

            let mut app = Box::<EguiApp>::default();
            app.texture_map().insert(
                twitch_chat_watcher::filter::MODERATOR_BADGE_NAME.to_owned(),
                cc.egui_ctx.load_texture(
                    twitch_chat_watcher::filter::MODERATOR_BADGE_NAME,
                    load_image_from_memory(include_bytes!("../assets/mod.png")).unwrap(),
                    TextureOptions::default(),
                ),
            );
            app.texture_map().insert(
                twitch_chat_watcher::filter::PARTNER_BADGE_NAME.to_owned(),
                cc.egui_ctx.load_texture(
                    twitch_chat_watcher::filter::PARTNER_BADGE_NAME,
                    load_image_from_memory(include_bytes!("../assets/partner.png")).unwrap(),
                    TextureOptions::default(),
                ),
            );
            app.texture_map().insert(
                twitch_chat_watcher::filter::VIP_BADGE_NAME.to_owned(),
                cc.egui_ctx.load_texture(
                    twitch_chat_watcher::filter::VIP_BADGE_NAME,
                    load_image_from_memory(include_bytes!("../assets/vip.png")).unwrap(),
                    TextureOptions::default(),
                ),
            );
            app.texture_map().insert(
                twitch_chat_watcher::filter::BROADCASTER_BADGE_NAME.to_owned(),
                cc.egui_ctx.load_texture(
                    twitch_chat_watcher::filter::BROADCASTER_BADGE_NAME,
                    load_image_from_memory(include_bytes!("../assets/broadcaster.png")).unwrap(),
                    TextureOptions::default(),
                ),
            );

            if let Some(storage) = cc.storage {
                if let Some(data_str) = storage.get_string(APP_SAVE_STATE_KEY) {
                    if let Ok(state) = ron::from_str::<AppSaveState>(&data_str) {
                        if app.restore(&state, &cc.egui_ctx).is_err() {
                            log::error!("Load save state failed");
                        }
                    }
                }
            }

            app
        }),
    )
}

fn load_image_from_memory(image_data: &[u8]) -> Result<ColorImage, image::ImageError> {
    let image = image::load_from_memory(image_data)?;
    let size = [
        image.width().try_into().unwrap(),
        image.height().try_into().unwrap(),
    ];
    let image_buffer = image.to_rgba8();
    let pixels = image_buffer.as_flat_samples();
    Ok(ColorImage::from_rgba_unmultiplied(size, pixels.as_slice()))
}
