use crate::{
    audio_player::AlertPlayer,
    chat_client::{self, IrcClient, TwitchMsg},
    filter::Filter,
};
extern crate lab;
use super::{
    chat_client::ChannelConnectionState, chat_client::ChannelManager, filter::FilterState,
    ASYNC_RUNTIME,
};
use arboard::Clipboard;
use cached::proc_macro::cached;
use chrono::{DateTime, Utc};
use eframe::{
    egui::{
        self, ComboBox, Context, DragValue, FontData, FontDefinitions, FontFamily::*, FontId,
        InnerResponse, Key, Label, Layout, Modifiers, Response, RichText, ScrollArea, Sense,
        Slider, Style, TextEdit, TextFormat, TextStyle, Ui,
    },
    emath::Align,
    epaint::{
        pos2,
        text::{LayoutJob, TextWrapping},
        vec2, Color32, FontFamily, Rect, Stroke, TextureHandle, Vec2,
    },
};
use font_loader::system_fonts;
use git_version::git_version;
use lab::Lab;
use once_cell::sync::Lazy;
use regex::Regex;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, sync::Arc, vec};

const ANONYMOUS_USERNAME: &str = "justinfan123";
const ANONYMOUS_PASSWORD: &str = "";
static FONT_LIST: Lazy<Vec<String>> = Lazy::new(|| {
    let mut v = vec!["".to_owned()];
    v.extend(system_fonts::query_all().into_iter());
    v.sort();
    v
});

#[derive(PartialEq)]
pub enum AppState {
    Normal,
    Config,
    ChannelList,
    ChannelConfig(usize, FilterState),
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq)]
enum NameDisplay {
    NickName,
    Id,
    Both,
}

pub struct EguiApp {
    state: AppState,
    username: String,
    access_token: String,
    new_channel_name: String,
    irc_client: Arc<tokio::sync::Mutex<IrcClient>>,
    channel_list: Vec<ChannelManager>,
    selected_channel: usize,
    def_filter: FilterState,
    error_msg: Option<String>,
    font_size: f32,
    textures: HashMap<String, TextureHandle>,
    use_twitch_color: bool,
    name_display: NameDisplay,
    show_sent_time: bool,
    readable_color_adjustment: bool,
    dark_theme: bool,
    log_btn: Option<(usize, bool)>,
    alert_volume: f32,
    alert_player: AlertPlayer,
    new_msg: String,
    credential_changed: bool,
    show_msg_id: Option<String>,
    reply_msg: Option<TwitchMsg>,
    selected_font: String,
    input_new_channel: bool,
    max_msg_count: usize,
    context_msg: Option<TwitchMsg>,
    last_time_updated: DateTime<Utc>,
    paused_messages: Option<Vec<TwitchMsg>>,
    paused_filtered_messages: Option<Vec<TwitchMsg>>,
}

impl Default for EguiApp {
    fn default() -> Self {
        Self {
            username: "".to_owned(),
            access_token: "".to_owned(),
            state: AppState::Normal,
            new_channel_name: String::new(),
            channel_list: vec![],
            selected_channel: 0,
            error_msg: None,
            font_size: super::DEFAULT_FONT_SIZE,
            def_filter: FilterState::default(),
            textures: HashMap::new(),
            use_twitch_color: true,
            name_display: NameDisplay::Both,
            show_sent_time: true,
            readable_color_adjustment: true,
            dark_theme: true,
            log_btn: None,
            alert_volume: 1.0,
            alert_player: AlertPlayer::default(),
            irc_client: Arc::new(tokio::sync::Mutex::new(
                ASYNC_RUNTIME
                    .block_on(async {
                        IrcClient::new(ANONYMOUS_USERNAME, ANONYMOUS_PASSWORD).await
                    })
                    .unwrap(),
            )),
            new_msg: String::new(),
            credential_changed: false,
            show_msg_id: None,
            reply_msg: None,
            selected_font: String::new(),
            input_new_channel: false,
            max_msg_count: super::MAX_MESSAGE_COUNT,
            context_msg: None,
            last_time_updated: Utc::now(),
            paused_messages: None,
            paused_filtered_messages: None,
        }
    }
}

impl EguiApp {
    pub fn new_channel(&mut self, channel_name: &str, filter: Filter) {
        let mut client = ChannelManager::new(
            self.irc_client.clone(),
            channel_name,
            self.max_msg_count,
            filter,
        );
        client.connect();
        self.channel_list.push(client);
        self.new_channel_name = "".to_owned();
        self.error_msg = None;
    }

    fn current_channel(&self) -> Option<&ChannelManager> {
        self.channel_list.get(self.selected_channel)
    }

    fn current_channel_mut(&mut self) -> Option<&mut ChannelManager> {
        self.channel_list.get_mut(self.selected_channel)
    }

    fn draw_config(&mut self, app_ui: &mut Ui, ctx: &Context) {
        let available_width = app_ui.available_width();
        app_ui.vertical(|ui| {
            ui.set_width(available_width);
            ScrollArea::vertical().show(ui, |ui| {
                ui.set_width(available_width);
                ui.add_space(10.0);
                if let Some(e) = &self.error_msg {
                    ui.label(RichText::new(e).color(Color32::RED));
                    ui.add_space(10.0);
                }
                ui.group(|ui| {
                    //let label = ui.label("Username: ");
                    //if ui
                    //    .text_edit_singleline(&mut self.username)
                    //    .labelled_by(label.id)
                    //    .changed()
                    //{
                    //    self.credential_changed = true;
                    //}
                    let label = ui.label("Access token: ");
                    if ui
                        .add(
                            TextEdit::singleline(&mut self.access_token)
                                .hint_text("oauth:xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"),
                        )
                        .labelled_by(label.id)
                        .changed()
                    {
                        self.credential_changed = true;
                    };
                    ui.hyperlink_to("Get access token from here", "https://twitchapps.com/tmi/");
                });
                ui.add_space(10.0);
                ComboBox::from_label("Select font")
                    .selected_text(self.selected_font.to_owned())
                    .width(300.0)
                    .show_ui(ui, |ui| {
                        for font in FONT_LIST.iter() {
                            if ui
                                .selectable_value(&mut self.selected_font, font.to_string(), font)
                                .clicked()
                            {
                                set_font(ui.ctx(), Some(&self.selected_font));
                            }
                        }
                    });

                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label("Font size:");
                    if ui
                        .add(DragValue::new(&mut self.font_size).speed(0.1))
                        .changed()
                    {
                        set_font_size(ctx, self.font_size);
                    }
                });
                ui.add_space(10.0);
                ui.checkbox(&mut self.use_twitch_color, "Use twitch username color");
                ui.add_space(10.0);
                ui.checkbox(&mut self.show_sent_time, "Show message sent time");
                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label("User display style: ");
                    ui.radio_value(&mut self.name_display, NameDisplay::Both, "Both");
                    ui.radio_value(
                        &mut self.name_display,
                        NameDisplay::NickName,
                        "Nickname only",
                    );
                    ui.radio_value(&mut self.name_display, NameDisplay::Id, "Id only");
                });
                ui.add_space(10.0);
                ui.checkbox(
                    &mut self.readable_color_adjustment,
                    "Adjust twitch username color for readability",
                );
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Use dark theme");
                    if toggle_btn(ui, &mut self.dark_theme).clicked() {
                        if self.dark_theme {
                            ctx.set_visuals(egui::Visuals::dark());
                        } else {
                            ctx.set_visuals(egui::Visuals::light());
                        }
                    };
                });
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.label("Alert volume: ");
                    if ui
                        .add(Slider::new(&mut self.alert_volume, 0.0..=2.0))
                        .changed()
                    {
                        self.alert_player.set_volume(self.alert_volume);
                    }
                    if ui.button("Test").clicked() {
                        self.alert_player.play().unwrap();
                    }
                });
                ui.add_space(10.0);
                ui.group(|ui| {
                    ui.label("Max message count:");
                    ui.add(DragValue::new(&mut self.max_msg_count).speed(20));
                });
                ui.add_space(10.0);
                ui.label("Default filter configurations");
                draw_filter_config(ui, &mut self.def_filter);
                ui.add_space(10.0);
                ui.label(format!("Version: {}", git_version!()));
            });
        });
    }

    fn draw_normal(&mut self, app_ui: &mut Ui, compact_mode: bool) {
        let main_area_available_size = app_ui.available_size();
        //eprintln!("1 {:?}", main_area_available_size);
        if compact_mode {
            app_ui.vertical(|ui| {
                //ui.add_space(main_area_available_size.y / 2.0);
                if !self.channel_list.is_empty() {
                    self.draw_chat(
                        ui,
                        vec2(main_area_available_size.x, main_area_available_size.y / 2.0),
                        true,
                    );
                    self.draw_chat(
                        ui,
                        vec2(main_area_available_size.x, main_area_available_size.y / 2.0),
                        false,
                    );
                    if let Some(c) = self.current_channel_mut() {
                        c.read();
                    }
                }
            });
        } else {
            app_ui.horizontal(|main_area_ui| {
                self.draw_channel_list(main_area_ui, vec2(300.0, main_area_available_size.y));
                main_area_ui.separator();
                let available_width = main_area_ui.available_width() - 50.0;
                if !self.channel_list.is_empty() {
                    self.draw_chat(
                        main_area_ui,
                        vec2(available_width / 2.0, main_area_available_size.y),
                        false,
                    );
                    main_area_ui.separator();
                    self.draw_chat(
                        main_area_ui,
                        vec2(available_width / 2.0, main_area_available_size.y),
                        true,
                    );
                    if let Some(c) = self.current_channel_mut() {
                        c.read();
                    }
                }
            });
        }
    }

    fn draw_channel_config(&mut self, app_ui: &mut Ui) {
        if let AppState::ChannelConfig(idx, filter_state) = &mut self.state {
            let available_width = app_ui.available_width();
            ScrollArea::vertical().show(app_ui, |ui| {
                ui.set_width(available_width);
                if let Some(e) = &self.error_msg {
                    ui.label(RichText::new(e).color(Color32::RED));
                }
                ui.add_space(10.0);
                let mut alert = self.channel_list[*idx].alert();
                if ui
                    .checkbox(&mut alert, "New filtered message alert")
                    .changed()
                {
                    if alert {
                        self.channel_list[*idx].set_alert(Some(self.alert_player.clone()));
                    } else {
                        self.channel_list[*idx].set_alert(None);
                    }
                }
                ui.add_space(10.0);
                draw_filter_config(ui, filter_state);
            });
        }
    }

    pub fn channel_list_mut(&mut self) -> &mut [ChannelManager] {
        &mut self.channel_list
    }

    pub fn restore(
        &mut self,
        save_state: &AppSaveState,
        ctx: &egui::Context,
    ) -> Result<(), regex::Error> {
        self.font_size = save_state.font_size;
        set_font_size(ctx, self.font_size);
        self.username = save_state.username.clone();
        self.access_token = save_state.access_token.clone();
        if !self.access_token.is_empty() && self.re_login().is_err() {
            self.error_msg = Some("Login to chat failed, using anonymous".to_string())
        }
        self.max_msg_count = save_state.max_msg_count;
        self.channel_list = save_state
            .channels
            .iter()
            .map(|save| {
                let mut client = ChannelManager::new(
                    self.irc_client.clone(),
                    save.name.clone(),
                    self.max_msg_count,
                    (&save.filter).try_into()?,
                );
                if save.enabled {
                    client.connect();
                }
                if let Some(log_path) = &save.log_status {
                    client.set_log(Some(log_path.clone()));
                }
                if let Some(log_path) = &save.filtered_log_status {
                    client.set_filtered_log(Some(log_path.clone()));
                }
                if save.alert {
                    client.set_alert(Some(self.alert_player.clone()));
                }
                Ok(client)
            })
            .collect::<Result<Vec<ChannelManager>, regex::Error>>()?;
        self.def_filter = save_state.def_filter.clone();
        self.show_sent_time = save_state.show_sent_time;
        self.use_twitch_color = save_state.use_twitch_color;
        self.name_display = save_state.name_display;
        self.readable_color_adjustment = save_state.readable_color_adjustment;
        self.dark_theme = save_state.dark_theme;
        if self.dark_theme {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }
        self.alert_volume = save_state.alert_volume;
        self.alert_player.set_volume(save_state.alert_volume);
        self.selected_channel = save_state.selected_channel;
        self.selected_font = save_state.selected_font.clone();
        set_font(ctx, Some(&self.selected_font));
        Ok(())
    }

    pub fn texture_map(&mut self) -> &mut HashMap<String, TextureHandle> {
        &mut self.textures
    }

    fn draw_chat(&mut self, ui: &mut Ui, size: Vec2, filtered: bool) {
        let mut end_pressed = false;
        let mut home_pressed = false;

        ui.input_mut(|input| {
            if input.focused {
                if input.key_pressed(Key::End) {
                    end_pressed = true;
                }
                if input.key_pressed(Key::Home) {
                    home_pressed = true;
                }
                if input.consume_key(Modifiers::default(), Key::Escape) {
                    self.reply_msg = None;
                }
            }
        });
        ui.vertical(|ui| {
            ui.set_max_height(size.y);
            ui.set_max_width(size.x);
            ui.group(|group_ui| {
                group_ui.vertical(|ui| {
                    //if filtered {
                    //    ui.set_height(size.y);
                    //} else {
                    //    ui.set_height(size.y - row_height - 20.0);
                    //}
                    ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                        if !filtered {
                            let is_connect = self.current_channel().unwrap().is_connected();
                            let mut edit = TextEdit::singleline(&mut self.new_msg)
                                .desired_width(f32::INFINITY)
                                .margin(vec2(0.0, 0.0))
                                .frame(true);
                            if !is_connect {
                                edit = edit
                                    .interactive(false)
                                    .hint_text("Channel is not connected");
                            }
                            if self.access_token.is_empty() {
                                edit = edit
                                    .interactive(false)
                                    .hint_text("Provide the access token to send message");
                            }
                            let response = ui.add(edit);
                            if response.lost_focus() {
                                ui.input_mut(|input| {
                                    if input.consume_key(Modifiers::default(), Key::Enter) {
                                        self.current_channel()
                                            .unwrap()
                                            .send_msg(self.new_msg.clone(), self.reply_msg.clone());
                                        self.new_msg = String::new();
                                        self.reply_msg = None;
                                    }
                                });
                            }
                            if !self.input_new_channel {
                                response.request_focus();
                            }
                            if let Some(reply_msg) = &self.reply_msg {
                                ui.add(Label::new(
                                    RichText::new(format!(
                                        "╭{}: {}",
                                        match self.name_display {
                                            NameDisplay::Both => format!(
                                                "{}({})",
                                                reply_msg.sender_display(),
                                                reply_msg.sender_login()
                                            ),
                                            NameDisplay::NickName =>
                                                reply_msg.sender_display().to_string(),
                                            NameDisplay::Id => reply_msg.sender_login().to_string(),
                                        },
                                        reply_msg.payload()
                                    ))
                                    .size(self.font_size * 0.8),
                                ))
                                .context_menu(|ui| {
                                    if ui.button("Cancel").clicked() {
                                        self.reply_msg = None;
                                    }
                                });
                            }
                            ui.add_space(1.0);
                            ui.separator();
                        }
                        ui.with_layout(Layout::top_down(Align::Center), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(if filtered {
                                    "Filtered message"
                                } else {
                                    "All message"
                                });
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    let log_status = if filtered {
                                        self.current_channel().unwrap().filtered_log_status()
                                    } else {
                                        self.current_channel().unwrap().log_status()
                                    };
                                    match log_status {
                                        None => {
                                            let response = ui.button("Log");
                                            if self.current_channel().unwrap().state()
                                                != ChannelConnectionState::Joined
                                            {
                                                response.on_hover_text(
                                                    "Please enable the channel first.",
                                                );
                                            } else if response.clicked() {
                                                self.log_btn =
                                                    Some((self.selected_channel, filtered));
                                            }
                                        }
                                        Some(r) => match r {
                                            Err(e) => {
                                                if ui
                                                    .button(
                                                        RichText::new("Stop").color(Color32::RED),
                                                    )
                                                    .on_hover_text(e)
                                                    .clicked()
                                                {
                                                    if filtered {
                                                        self.current_channel_mut()
                                                            .unwrap()
                                                            .set_filtered_log(None);
                                                    } else {
                                                        self.current_channel_mut()
                                                            .unwrap()
                                                            .set_log(None);
                                                    }
                                                }
                                            }
                                            Ok(p) => {
                                                if ui
                                                    .button(
                                                        RichText::new("Stop").color(Color32::GREEN),
                                                    )
                                                    .on_hover_text(format!("{}", p.display()))
                                                    .clicked()
                                                {
                                                    if filtered {
                                                        self.current_channel_mut()
                                                            .unwrap()
                                                            .set_filtered_log(None);
                                                    } else {
                                                        self.current_channel_mut()
                                                            .unwrap()
                                                            .set_log(None);
                                                    }
                                                }
                                            }
                                        },
                                    }

                                    if ui.button("Clear").clicked() {
                                        self.current_channel_mut().unwrap().clear_msg(filtered);
                                    }

                                    let is_paused = if filtered {
                                        self.paused_filtered_messages.is_some()
                                    } else {
                                        self.paused_messages.is_some()
                                    };
                                    if ui
                                        .button(if is_paused { "Resume" } else { "Pause" })
                                        .clicked()
                                    {
                                        if filtered {
                                            if is_paused {
                                                self.paused_filtered_messages = None;
                                            } else {
                                                self.paused_filtered_messages = Some(
                                                    self.current_channel()
                                                        .unwrap()
                                                        .get_msg(filtered),
                                                );
                                            }
                                        } else {
                                            if is_paused {
                                                self.paused_messages = None;
                                            } else {
                                                self.paused_messages = Some(
                                                    self.current_channel()
                                                        .unwrap()
                                                        .get_msg(filtered),
                                                );
                                            }
                                        }
                                    }

                                    ui.label(format!(
                                        "{} / {}",
                                        self.current_channel_mut().unwrap().get_msg_count(filtered),
                                        self.max_msg_count
                                    ));
                                });
                            });
                            ui.separator();

                            let mut highlight_message_found = self.show_msg_id.is_none();
                            ScrollArea::vertical()
                                .id_source(filtered)
                                //.auto_shrink([false, false])
                                .enable_scrolling(self.show_msg_id.is_none() || filtered)
                                //.auto_shrink([false; 2])
                                .stick_to_bottom(!home_pressed && self.show_msg_id.is_none())
                                .show(ui, |ui| {
                                    if home_pressed {
                                        ui.scroll_to_cursor(None);
                                    }
                                    {
                                        let messages = if filtered {
                                            if let Some(p) = self.paused_filtered_messages.as_ref()
                                            {
                                                p.clone()
                                            } else {
                                                self.current_channel().unwrap().get_msg(filtered)
                                            }
                                        } else {
                                            if let Some(p) = self.paused_messages.as_ref() {
                                                p.clone()
                                            } else {
                                                self.current_channel().unwrap().get_msg(filtered)
                                            }
                                        };
                                        for msg in messages {
                                            if !filtered {
                                                if let Some(id) = &self.show_msg_id {
                                                    if id == msg.id() {
                                                        ui.scroll_to_cursor(Some(Align::Center));
                                                        highlight_message_found = true;
                                                    }
                                                }
                                            }
                                            self.draw_msg(ui, &msg);
                                        }
                                    }
                                    ui.input(|i| {
                                        if i.pointer.button_clicked(egui::PointerButton::Primary) {
                                            self.show_msg_id = None;
                                        }
                                    });
                                    if end_pressed {
                                        ui.scroll_to_cursor(None);
                                    }
                                });
                            if !filtered && !highlight_message_found {
                                self.show_msg_id = None;
                            }
                        });
                    });

                    //area.show_rows(ui, row_height, num_rows, |ui, row_range| {
                    //    let client = &self.current_channel();
                    //    for msg in client.get_n_msg(row_range, filtered) {
                    //        self.draw_msg(ui, &msg, font_size);
                    //    }
                    //});
                });
            });
        });
    }

    fn draw_msg(&mut self, ui: &mut Ui, msg: &TwitchMsg) -> InnerResponse<()> {
        let text_style = TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + 1.0;
        let highlight = if let Some(id) = &self.show_msg_id {
            id == msg.id()
        } else {
            false
        };
        let bg_color = if highlight {
            Color32::BROWN
        } else {
            ui.visuals().panel_fill
        };
        let text_color = if self.readable_color_adjustment {
            adjust_readable_color(ui.visuals().text_color(), bg_color)
        } else {
            ui.visuals().text_color()
        };
        let mut username_color = if self.use_twitch_color {
            msg.name_color()
                .map(|c| Color32::from_rgb(c[0], c[1], c[2]))
                .unwrap_or(ui.style().visuals.warn_fg_color)
        } else {
            ui.style().visuals.warn_fg_color
        };

        if self.readable_color_adjustment {
            username_color = adjust_readable_color(username_color, bg_color);
        }
        if let Some(((reply_author_id, reply_author_name), reply_msg_body)) = msg
            .tag("reply-parent-user-login")
            .zip(msg.tag("reply-parent-display-name"))
            .zip(msg.tag("reply-parent-msg-body"))
        {
            ui.horizontal(|ui| {
                if ui
                    .add(
                        Label::new(
                            RichText::new(format!(
                                "╭{}: {}",
                                match self.name_display {
                                    NameDisplay::Both =>
                                        format!("{}({})", reply_author_name, reply_author_id),
                                    NameDisplay::NickName => reply_author_name.clone(),
                                    NameDisplay::Id => reply_author_id.clone(),
                                },
                                reply_msg_body
                            ))
                            .size(self.font_size * 0.8),
                        )
                        .sense(Sense::click()),
                    )
                    .clicked()
                {
                    self.show_msg_id = msg.tag("reply-parent-msg-id").cloned();
                }
            });
        }
        let main_space = ui.horizontal_wrapped(|ui| {
            let time_str;
            let message;
            let mut items = vec![];
            if self.show_sent_time {
                let local_time = msg.sent_time().unwrap().with_timezone(&chrono::Local);
                //ui.label(local_time.format("%H:%M:%S").to_string());
                time_str = local_time.format("%H:%M:%S ").to_string();
                items.push(DisplayItem::Text(
                    &time_str,
                    Some(text_color),
                    Some(bg_color),
                ));
            }
            for (badge_name, _) in msg.badges().iter() {
                if badge_name == super::filter::BROADCASTER_BADGE_NAME {
                    //ui.image(
                    //    self.textures
                    //        .get(super::filter::BROADCASTER_BADGE_NAME)
                    //        .unwrap(),
                    //    vec2(row_height, row_height),
                    //);
                    items.push(DisplayItem::Image(
                        self.textures
                            .get(super::filter::BROADCASTER_BADGE_NAME)
                            .unwrap()
                            .clone(),
                    ));
                }
                if badge_name == super::filter::MODERATOR_BADGE_NAME {
                    //ui.image(
                    //    self.textures
                    //        .get(super::filter::MODERATOR_BADGE_NAME)
                    //        .unwrap(),
                    //    vec2(row_height, row_height),
                    //);
                    items.push(DisplayItem::Image(
                        self.textures
                            .get(super::filter::MODERATOR_BADGE_NAME)
                            .unwrap()
                            .clone(),
                    ));
                }
                if badge_name == super::filter::PARTNER_BADGE_NAME {
                    //ui.image(
                    //    self.textures
                    //        .get(super::filter::PARTNER_BADGE_NAME)
                    //        .unwrap(),
                    //    vec2(row_height, row_height),
                    //);
                    items.push(DisplayItem::Image(
                        self.textures
                            .get(super::filter::PARTNER_BADGE_NAME)
                            .unwrap()
                            .clone(),
                    ));
                }
                if badge_name == super::filter::VIP_BADGE_NAME {
                    //ui.image(
                    //    self.textures.get(super::filter::VIP_BADGE_NAME).unwrap(),
                    //    vec2(row_height, row_height),
                    //);
                    items.push(DisplayItem::Image(
                        self.textures
                            .get(super::filter::VIP_BADGE_NAME)
                            .unwrap()
                            .clone(),
                    ));
                }
            }
            //let mut layout = LayoutJob {
            //    wrap: TextWrapping {
            //        max_width: ui.available_width(),
            //        break_anywhere: true,
            //        ..Default::default()
            //    },
            //    ..Default::default()
            //};
            //let format: TextFormat = TextFormat {
            //    color: text_color,
            //    background: bg_color,
            //    font_id: TextStyle::Body.resolve(ui.style()),
            //    ..Default::default()
            //};
            let name = match self.name_display {
                NameDisplay::Both => format!("{}({})", msg.sender_display(), msg.sender_login()),
                NameDisplay::NickName => msg.sender_display().to_owned(),
                NameDisplay::Id => msg.sender_login().to_owned(),
            };
            //layout.append(
            //    &name,
            //    0.0,
            //    TextFormat {
            //        color,
            //        ..format.clone()
            //    },
            //);
            items.push(DisplayItem::Text(
                &name,
                Some(username_color),
                Some(bg_color),
            ));
            //layout.append(": ", 0.0, format.clone());
            //layout.append(msg.payload().trim(), 0.0, format);
            message = format!(": {}", msg.payload().trim());
            items.push(DisplayItem::Text(
                &message,
                Some(text_color),
                Some(bg_color),
            ));

            let response: Response = draw_text_and_image(ui, items, ui.available_width(), 5.0);
            if response.is_pointer_button_down_on() {
                self.context_msg = Some(msg.clone());
            }
            response.context_menu(|ui| {
                let msg = self.context_msg.clone().unwrap();
                ui.set_width(400.0);
                ui.hyperlink_to(
                    format!("{}({})", msg.sender_display(), msg.sender_login()),
                    format!("https://www.twitch.tv/{}", msg.sender_login()),
                );
                ui.separator();
                let mut drew_badge = false;
                for (badge_name, _) in msg.badges().iter() {
                    if badge_name == super::filter::BROADCASTER_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::BROADCASTER_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Broadcaster");
                        });
                        drew_badge = true;
                    }
                    if badge_name == super::filter::MODERATOR_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::MODERATOR_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Moderator");
                        });
                        drew_badge = true;
                    }
                    if badge_name == super::filter::PARTNER_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::PARTNER_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Partner");
                        });
                        drew_badge = true;
                    }
                    if badge_name == super::filter::VIP_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self.textures.get(super::filter::VIP_BADGE_NAME).unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("VIP");
                        });
                        drew_badge = true;
                    }
                }
                if drew_badge {
                    ui.separator();
                }
                if ui.button("Reply to this").clicked() {
                    self.reply_msg = Some(msg.clone());
                    ui.close_menu();
                }
                if ui.button("Add user to filter").clicked() {
                    match Regex::new(&regex::escape(msg.sender_login())) {
                        Ok(r) => self
                            .current_channel_mut()
                            .unwrap()
                            .mut_filter(|f| f.add_author_pat(r)),
                        Err(e) => self.error_msg = Some(format!("{}", e)),
                    }
                    ui.close_menu();
                }
                if ui.button("Add user to exclusive filter").clicked() {
                    match Regex::new(&regex::escape(msg.sender_login())) {
                        Ok(r) => self
                            .current_channel_mut()
                            .unwrap()
                            .mut_filter(|f| f.add_exc_author_pat(r)),
                        Err(e) => self.error_msg = Some(format!("{}", e)),
                    }
                    ui.close_menu();
                }
                if ui.button("Copy content").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(msg.payload().trim()).unwrap();
                    ui.close_menu();
                }
                if ui.button("Copy sender's nickname").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(msg.sender_display()).unwrap();
                    ui.close_menu();
                }
                if ui.button("Copy sender's id").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(msg.sender_login()).unwrap();
                    ui.close_menu();
                }
            });
        });
        ui.separator();
        main_space
    }

    fn draw_channel_list(&mut self, ui: &mut Ui, size: Vec2) {
        let mut remove_channel = None;

        ui.vertical(|channel_list_ui| {
            channel_list_ui.set_height(size.y);
            channel_list_ui.set_width(size.x);
            ScrollArea::vertical().show(channel_list_ui, |channel_list_ui| {
                for (idx, client) in self.channel_list.iter_mut().enumerate() {
                    let bg_color = if self.selected_channel == idx {
                        channel_list_ui.visuals().selection.bg_fill
                    } else {
                        channel_list_ui.visuals().panel_fill
                    };
                    let text_color = match client.state() {
                        ChannelConnectionState::Uninitialized => Color32::GRAY,
                        ChannelConnectionState::Joined => {
                            if client.log_status().is_some()
                                || client.filtered_log_status().is_some()
                            {
                                Color32::BLUE
                            } else {
                                Color32::GREEN
                            }
                        }
                    };
                    channel_list_ui.horizontal(|channel_ui| {
                        if channel_ui
                            .selectable_value(
                                &mut self.selected_channel,
                                idx,
                                RichText::new(client.channel_name()).color(
                                    if self.readable_color_adjustment {
                                        adjust_readable_color(text_color, bg_color)
                                    } else {
                                        text_color
                                    },
                                ),
                            )
                            .context_menu(|ui| {
                                ui.hyperlink_to(
                                    "Twitch page",
                                    format!("https://www.twitch.tv/{}", client.channel_name()),
                                );
                                ui.separator();
                                if ui.button("Configuration").clicked() {
                                    self.state =
                                        AppState::ChannelConfig(idx, client.get_filter_state());
                                    ui.close_menu();
                                }
                                if ui.button("Delete").clicked() {
                                    remove_channel = Some(idx);
                                    ui.close_menu();
                                }
                            })
                            .clicked()
                        {
                            self.state = AppState::Normal;
                        };

                        channel_ui.with_layout(Layout::right_to_left(Align::RIGHT), |sub_ui| {
                            let mut switch = client.is_connected();
                            if toggle_btn(sub_ui, &mut switch).changed() {
                                if switch {
                                    client.connect()
                                } else {
                                    client.disconnect();
                                }
                            }
                            if client.has_unread_filtered_msg() {
                                sub_ui.label(
                                    RichText::new("!").color(sub_ui.style().visuals.warn_fg_color),
                                );
                            }
                            sub_ui.label(format!(
                                "{} / {}",
                                client.get_msg_count(false),
                                client.get_msg_count(true)
                            ));
                        });
                    });

                    channel_list_ui.separator();
                }
                channel_list_ui.add_space(5.0);
                channel_list_ui.with_layout(Layout::right_to_left(Align::Min), |new_channel_ui| {
                    if new_channel_ui.button("+").clicked() {
                        self.new_channel(
                            &self.new_channel_name.clone(),
                            (&self.def_filter).try_into().unwrap(),
                        );
                    }
                    let edit = TextEdit::singleline(&mut self.new_channel_name)
                        .desired_width(f32::INFINITY);
                    let response = new_channel_ui.add(edit);
                    if response.lost_focus() {
                        new_channel_ui.input_mut(|input| {
                            if input.consume_key(Modifiers::default(), Key::Enter) {
                                self.new_channel(
                                    &self.new_channel_name.clone(),
                                    (&self.def_filter).try_into().unwrap(),
                                );
                            }
                        });
                        self.input_new_channel = false;
                    }
                    if response.gained_focus() {
                        self.input_new_channel = true;
                    }
                });
                if let Some(err) = &self.error_msg {
                    channel_list_ui.label(RichText::new(err).color(Color32::RED));
                }
            });
        });
        if let Some(idx) = remove_channel {
            self.channel_list.remove(idx);
            if self.selected_channel >= idx && idx > 0 {
                self.selected_channel -= 1;
            }
        }
    }

    fn re_login(&mut self) -> Result<(), ()> {
        let new_client: IrcClient = ASYNC_RUNTIME.block_on(async {
            if !self.access_token.is_empty() {
                // seems twitch irc server doesn't care about username.
                chat_client::IrcClient::new("a", &self.access_token).await
            } else {
                chat_client::IrcClient::new(ANONYMOUS_USERNAME, ANONYMOUS_PASSWORD).await
            }
        })?;
        ASYNC_RUNTIME.block_on(async {
            *self.irc_client.lock().await = new_client;
        });

        for channel in self.channel_list.iter_mut() {
            if channel.is_connected() {
                channel.connect();
            }
        }
        Ok(())
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Utc::now();
        if now - self.last_time_updated < chrono::Duration::milliseconds(100) {
            std::thread::sleep(std::time::Duration::from_millis(
                100 - (now.timestamp_millis() - self.last_time_updated.timestamp_millis()) as u64,
            ));
        }
        if let Some((idx, filtered)) = self.log_btn {
            if let Some(path) = FileDialog::new().save_file() {
                if filtered {
                    self.channel_list[idx].set_filtered_log(Some(path));
                } else {
                    self.channel_list[idx].set_log(Some(path));
                }
            }
            self.log_btn = None;
        }
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
        egui::CentralPanel::default().show(ctx, |app_ui| {
            let compact_mode = app_ui.available_height() / app_ui.available_width() > 0.9;
            app_ui.horizontal(|ui| {
                if let AppState::ChannelConfig(idx, filter_state) = &self.state {
                    if ui.button("Back").clicked() {
                        match self.channel_list[*idx].set_filter(filter_state) {
                            Ok(_) => {
                                self.error_msg = None;
                                self.state = AppState::Normal;
                            }
                            Err(e) => self.error_msg = Some(format!("{}", e)),
                        }
                    }
                    ui.separator();
                }
                if ui
                    .selectable_label(
                        self.state == AppState::Config,
                        if self.state == AppState::Config {
                            "Back"
                        } else {
                            "Configuration"
                        },
                    )
                    .clicked()
                {
                    match &self.state {
                        AppState::Config => {
                            if self.credential_changed && self.re_login().is_err() {
                                self.error_msg = Some("Login to chat failed".to_string());
                            } else if let Err(e) = build_regexes(&self.def_filter.inc_author) {
                                self.error_msg = Some(format!("{}", e));
                            } else if let Err(e) = build_regexes(&self.def_filter.inc_msg) {
                                self.error_msg = Some(format!("{}", e));
                            } else if let Err(e) = build_regexes(&self.def_filter.exc_msg) {
                                self.error_msg = Some(format!("{}", e));
                            } else if let Err(e) = build_regexes(&self.def_filter.exc_author) {
                                self.error_msg = Some(format!("{}", e));
                            } else {
                                self.error_msg = None;
                                self.state = AppState::Normal;
                                for channel in self.channel_list.iter_mut() {
                                    channel.set_max_msg_count(self.max_msg_count);
                                }
                            }
                        }
                        _ => {
                            self.error_msg = None;
                            self.state = AppState::Config;
                            self.credential_changed = false;
                        }
                    }
                }
                ui.separator();
                if compact_mode
                    && (self.state == AppState::Normal || self.state == AppState::ChannelList)
                {
                    let mut layout_job = LayoutJob::default();
                    layout_job.append(
                        "Channel list",
                        0.0,
                        TextFormat {
                            font_id: TextStyle::Body.resolve(ui.style()),
                            ..Default::default()
                        },
                    );
                    if self
                        .channel_list
                        .iter()
                        .any(|c| c.has_unread_filtered_msg())
                    {
                        layout_job.append(
                            "!",
                            5.0,
                            TextFormat {
                                color: ui.style().visuals.warn_fg_color,
                                font_id: TextStyle::Body.resolve(ui.style()),
                                ..Default::default()
                            },
                        );
                    }
                    if ui
                        .selectable_label(self.state == AppState::ChannelList, layout_job)
                        .clicked()
                    {
                        if self.state != AppState::ChannelList {
                            self.state = AppState::ChannelList;
                        } else {
                            self.state = AppState::Normal;
                        }
                    }

                    if let Some(channel) = self.current_channel() {
                        ui.separator();
                        ui.hyperlink_to(
                            format!("#{}", channel.channel_name()),
                            format!("https://www.twitch.tv/{}", channel.channel_name()),
                        );
                    }
                }
            });
            app_ui.separator();
            match &mut self.state {
                AppState::Normal => self.draw_normal(app_ui, compact_mode),
                AppState::Config => self.draw_config(app_ui, ctx),
                AppState::ChannelConfig(_, _) => self.draw_channel_config(app_ui),
                AppState::ChannelList => self.draw_channel_list(app_ui, app_ui.available_size()),
            }
        });
        self.last_time_updated = now;
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        storage.set_string(
            super::APP_SAVE_STATE_KEY,
            ron::to_string(&AppSaveState::from(self as &EguiApp)).unwrap(),
        );
    }
}

fn build_regexes(patterns: &str) -> Result<Vec<Regex>, regex::Error> {
    let mut out = vec![];
    for line in patterns
        .split('\n')
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
    {
        out.push(Regex::new(line)?);
    }
    Ok(out)
}
pub fn set_font_size(ctx: &Context, size: f32) {
    let arc: &Style = &ctx.style();
    let mut style = arc.clone();

    style
        .text_styles
        .insert(TextStyle::Body, FontId::new(size, Proportional));
    style
        .text_styles
        .insert(TextStyle::Button, FontId::new(size, Proportional));
    style
        .text_styles
        .insert(TextStyle::Heading, FontId::new(size + 12.0, Proportional));
    style.wrap = Some(true);
    ctx.set_style(style)
}

#[derive(Deserialize, Serialize)]
struct ChannelSaveState {
    name: String,
    enabled: bool,
    filter: FilterState,
    bell: bool,
    log_status: Option<PathBuf>,
    filtered_log_status: Option<PathBuf>,
    alert: bool,
}

#[derive(Deserialize, Serialize)]
pub struct AppSaveState {
    font_size: f32,
    channels: Vec<ChannelSaveState>,
    def_filter: FilterState,
    use_twitch_color: bool,
    name_display: NameDisplay,
    show_sent_time: bool,
    username: String,
    access_token: String,
    readable_color_adjustment: bool,
    dark_theme: bool,
    alert_volume: f32,
    selected_channel: usize,
    selected_font: String,
    max_msg_count: usize,
}

impl From<&EguiApp> for AppSaveState {
    fn from(value: &EguiApp) -> Self {
        Self {
            font_size: value.font_size,
            channels: value
                .channel_list
                .iter()
                .map(|c| ChannelSaveState {
                    name: c.channel_name().to_owned(),
                    enabled: c.is_connected(),
                    filter: c.get_filter_state(),
                    log_status: c.log_status().and_then(|r| r.ok()),
                    filtered_log_status: c.filtered_log_status().and_then(|r| r.ok()),
                    bell: false,
                    alert: c.alert(),
                })
                .collect(),
            def_filter: value.def_filter.clone(),
            use_twitch_color: value.use_twitch_color,
            name_display: value.name_display,
            show_sent_time: value.show_sent_time,
            username: value.username.clone(),
            access_token: value.access_token.clone(),
            readable_color_adjustment: value.readable_color_adjustment,
            dark_theme: value.dark_theme,
            alert_volume: value.alert_volume,
            selected_channel: value.selected_channel,
            selected_font: value.selected_font.clone(),
            max_msg_count: value.max_msg_count,
        }
    }
}

fn toggle_btn(ui: &mut egui::Ui, on: &mut bool) -> egui::Response {
    let desired_size = ui.spacing().interact_size.y * egui::vec2(2.0, 1.0);
    let (rect, mut response) = ui.allocate_exact_size(desired_size, egui::Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    response.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Checkbox, *on, ""));

    if ui.is_rect_visible(rect) {
        let how_on = ui.ctx().animate_bool(response.id, *on);
        let visuals = ui.style().interact_selectable(&response, *on);
        let rect: eframe::epaint::Rect = rect.expand(visuals.expansion);
        let radius = 0.5 * rect.height();
        ui.painter()
            .rect(rect, radius, visuals.bg_fill, visuals.bg_stroke);
        let circle_x = egui::lerp((rect.left() + radius)..=(rect.right() - radius), how_on);
        let center = egui::pos2(circle_x, rect.center().y);
        ui.painter()
            .circle(center, 0.75 * radius, visuals.bg_fill, visuals.fg_stroke);
    }

    response
}

fn draw_filter_config(ui: &mut Ui, filter_state: &mut FilterState) {
    ui.group(|group_ui| {
        let label: Response = group_ui.label("Inclusive Message Filters: ");
        group_ui
            .add(TextEdit::multiline(&mut filter_state.inc_msg).desired_width(500.0))
            .labelled_by(label.id);
    });
    ui.add_space(10.0);
    ui.group(|group_ui| {
        let label = group_ui.label("Inclusive Author Filters (only test against user id)");
        group_ui
            .add(TextEdit::multiline(&mut filter_state.inc_author).desired_width(500.0))
            .labelled_by(label.id);
    });
    ui.add_space(10.0);
    ui.group(|group_ui| {
        let label: Response = group_ui.label("Exclusive Message Filters: ");
        group_ui
            .add(TextEdit::multiline(&mut filter_state.exc_msg).desired_width(500.0))
            .labelled_by(label.id);
    });
    ui.add_space(10.0);
    ui.group(|group_ui| {
        let label = group_ui.label("Exclusive Author Filters (only test against user id)");
        group_ui
            .add(TextEdit::multiline(&mut filter_state.exc_author).desired_width(500.0))
            .labelled_by(label.id);
    });
    ui.add_space(10.0);
    ui.checkbox(&mut filter_state.broadcaster, "Broadcaster");
    ui.checkbox(&mut filter_state.moderator, "Moderator");
    ui.checkbox(&mut filter_state.vip, "VIP");
    ui.checkbox(&mut filter_state.partner, "Partner");
}

#[cached]
fn adjust_readable_color(fg: Color32, bg: Color32) -> Color32 {
    let mut color = fg;
    let bg_lab = Lab::from_rgb(&[bg.r(), bg.g(), bg.b()]);
    let mut fg_lab = Lab::from_rgb(&[fg.r(), fg.g(), fg.b()]);
    let l_delta = if bg_lab.l > fg_lab.l {
        bg_lab.l - fg_lab.l
    } else {
        fg_lab.l - bg_lab.l
    };
    if l_delta < 35.0 {
        if bg_lab.l < 50.0 {
            fg_lab.l = bg_lab.l + 50.0;
        } else {
            fg_lab.l = bg_lab.l - 50.0;
        }
        let new_rgb = fg_lab.to_rgb();
        color = Color32::from_rgb(new_rgb[0], new_rgb[1], new_rgb[2]);
    }
    color
}

pub fn set_font(ctx: &Context, mut font_family: Option<&str>) {
    let mut fonts = FontDefinitions::default();
    fonts.font_data.insert(
        "NotoMerged".to_owned(),
        FontData::from_static(include_bytes!("../fonts/NotoSansMerged-Regular.otf")),
    );
    if font_family == Some("") {
        font_family = None;
    }
    if let Some(family) = font_family {
        let property = system_fonts::FontPropertyBuilder::new()
            .family(family)
            .build();
        let (font_data, _) = system_fonts::get(&property).unwrap();
        fonts
            .font_data
            .insert(family.to_owned(), FontData::from_owned(font_data));
        fonts
            .families
            .get_mut(&FontFamily::Proportional)
            .unwrap()
            .insert(0, family.to_owned());
    }
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(
            if font_family.is_some() { 1 } else { 0 },
            "NotoMerged".to_owned(),
        );
    ctx.set_fonts(fonts);
}

enum DisplayItem<'a> {
    Text(&'a str, Option<Color32>, Option<Color32>),
    Image(TextureHandle),
}

fn draw_text_and_image(
    ui: &mut Ui,
    items: Vec<DisplayItem>,
    max_width: f32,
    image_margin: f32,
) -> Response {
    let start_pos = ui.cursor().min;
    let mut cursor_pos = start_pos;
    let text_style = TextStyle::Body;
    let row_height = ui.text_style_height(&text_style) + 1.0;
    let text_format = TextFormat {
        font_id: ui.style().text_styles.get(&text_style).unwrap().clone(),
        ..Default::default()
    };

    {
        let painter = ui.painter();
        for item in items.into_iter() {
            match item {
                DisplayItem::Image(t) => {
                    if cursor_pos.x + row_height + image_margin - start_pos.x > max_width {
                        cursor_pos.y += row_height;
                        cursor_pos.x = start_pos.x;
                        painter.image(
                            (&t).into(),
                            Rect::from_min_max(
                                cursor_pos,
                                pos2(cursor_pos.x + row_height, cursor_pos.y + row_height),
                            ),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        cursor_pos.x += row_height + image_margin;
                    } else {
                        cursor_pos.x += image_margin;
                        painter.image(
                            (&t).into(),
                            Rect::from_min_max(
                                cursor_pos,
                                pos2(cursor_pos.x + row_height, cursor_pos.y + row_height),
                            ),
                            Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                            Color32::WHITE,
                        );
                        cursor_pos.x += row_height + image_margin;
                    }
                }
                DisplayItem::Text(text, fg, bg) => {
                    let mut layout = LayoutJob::default();
                    layout.wrap = TextWrapping {
                        max_width: max_width,
                        break_anywhere: true,
                        ..Default::default()
                    };
                    layout.append(
                        text,
                        cursor_pos.x - start_pos.x,
                        TextFormat {
                            color: fg.unwrap_or(ui.visuals().text_color()),
                            background: bg.unwrap_or(ui.visuals().panel_fill),
                            ..text_format.clone()
                        },
                    );
                    let mut galley = None;
                    ui.fonts(|fonts| {
                        galley = Some(fonts.layout_job(layout));
                    });
                    let mut next_pos = cursor_pos;
                    let galley = galley.unwrap();
                    next_pos.x = start_pos.x + galley.rows.last().unwrap().rect.max.x;
                    next_pos.y += (galley.rows.len() as f32 - 1.0) * row_height;
                    painter.galley(pos2(start_pos.x, cursor_pos.y), galley);
                    cursor_pos = next_pos;
                }
            }
        }
    }
    let (_rect, response) = ui.allocate_exact_size(
        vec2(max_width, cursor_pos.y - start_pos.y + row_height),
        Sense::click(),
    );
    let painter = ui.painter();
    if response.hovered() {
        let mut i = start_pos.y;
        while i < cursor_pos.y {
            painter.hline(
                start_pos.x..=start_pos.x + max_width,
                i + row_height,
                Stroke {
                    width: 1.0,
                    color: ui.visuals().text_color(),
                },
            );
            i += row_height;
        }
        painter.hline(
            start_pos.x..=cursor_pos.x,
            i + row_height,
            Stroke {
                width: 1.0,
                color: ui.visuals().text_color(),
            },
        );
    }
    response
}
