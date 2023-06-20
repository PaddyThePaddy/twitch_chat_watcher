use crate::{audio_player::AlertPlayer, filter::Filter};
extern crate lab;
use super::{chat_client::ChannelConnectionState, chat_client::ChatClient, filter::FilterState};
use arboard::Clipboard;
use cached::proc_macro::cached;
use eframe::{
    egui::{
        self, Context, DragValue, FontFamily::*, FontId, Key, Label, Layout, Modifiers, Response,
        RichText, ScrollArea, Slider, Style, TextEdit, TextFormat, TextStyle, Ui,
    },
    emath::Align,
    epaint::{
        text::{LayoutJob, TextWrapping},
        vec2, Color32, TextureHandle, Vec2,
    },
};
use lab::Lab;
use regex::Regex;
use rfd::FileDialog;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, path::PathBuf, time::Duration, vec};
use twitch_irc::message::PrivmsgMessage;

pub enum AppState {
    Normal,
    Config,
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
    channel_list: Vec<ChatClient>,
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
            alert_player: AlertPlayer::new(),
        }
    }
}

impl EguiApp {
    pub fn new_channel(&mut self, channel_name: &str, filter: Filter) {
        let mut client = ChatClient::new(channel_name, 1000, filter);
        match client.connect() {
            Ok(_) => {
                self.channel_list.push(client);
                self.new_channel_name = "".to_owned();
                self.error_msg = None;
            }
            Err(e) => self.error_msg = Some(format!("{}", e)),
        }
    }

    fn draw_config(&mut self, app_ui: &mut Ui, ctx: &Context) {
        let available_width = app_ui.available_width();
        app_ui.vertical(|ui| {
            ui.set_width(available_width);
            ScrollArea::vertical().show(ui, |ui| {
                ui.set_width(available_width);
                if let Some(e) = &self.error_msg {
                    ui.label(RichText::new(e).color(Color32::RED));
                    ui.add_space(10.0);
                }
                //ui.group(|ui| {
                //    let label = ui.label("Username: ");
                //    ui.text_edit_singleline(&mut self.username)
                //        .labelled_by(label.id);
                //    let label = ui.label("Access token: ");
                //    ui.text_edit_singleline(&mut self.access_token)
                //        .labelled_by(label.id);
                //});
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
                ui.label("Default filter configurations");
                draw_filter_config(ui, &mut self.def_filter);
            });
        });
    }

    fn draw_normal(&mut self, app_ui: &mut Ui) {
        let main_area_available_size = app_ui.available_size();
        let mut remove_channel = None;
        let uninitialized_color = if self.readable_color_adjustment {
            adjust_readable_color(Color32::GRAY, app_ui.visuals().panel_fill)
        } else {
            Color32::GRAY
        };
        let joined_color = if self.readable_color_adjustment {
            adjust_readable_color(Color32::GREEN, app_ui.visuals().panel_fill)
        } else {
            Color32::GREEN
        };
        let logging_color = if self.readable_color_adjustment {
            adjust_readable_color(Color32::BLUE, app_ui.visuals().panel_fill)
        } else {
            Color32::BLUE
        };

        app_ui.horizontal(|main_area_ui| {
            main_area_ui.vertical(|channel_list_ui| {
                channel_list_ui.set_height(main_area_available_size.y);
                channel_list_ui.set_width(300.0);
                ScrollArea::vertical().show(channel_list_ui, |channel_list_ui| {
                    for (idx, client) in self.channel_list.iter_mut().enumerate() {
                        channel_list_ui.horizontal(|channel_ui| {
                            channel_ui
                                .radio_value(
                                    &mut self.selected_channel,
                                    idx,
                                    RichText::new(client.channel_name()).color(
                                        match client.state() {
                                            ChannelConnectionState::Uninitialized => {
                                                uninitialized_color
                                            }
                                            ChannelConnectionState::Joined => {
                                                if client.log_status().is_some()
                                                    || client.filtered_log_status().is_some()
                                                {
                                                    logging_color
                                                } else {
                                                    joined_color
                                                }
                                            }
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
                                });

                            channel_ui.with_layout(Layout::right_to_left(Align::RIGHT), |sub_ui| {
                                let mut switch = client.is_connected();
                                if toggle_btn(sub_ui, &mut switch).changed() {
                                    if switch {
                                        match client.connect() {
                                            Ok(()) => {}
                                            Err(e) => {
                                                self.error_msg = Some(format!("{}", e));
                                            }
                                        }
                                    } else {
                                        client.disconnect();
                                    }
                                }
                                if client.has_unread_filtered_msg() {
                                    sub_ui.label(
                                        RichText::new("!")
                                            .color(sub_ui.style().visuals.warn_fg_color),
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
                    channel_list_ui.horizontal(|new_channel_ui| {
                        let response =
                            new_channel_ui.text_edit_singleline(&mut self.new_channel_name);
                        if response.lost_focus() {
                            new_channel_ui.input_mut(|input| {
                                if input.consume_key(Modifiers::default(), Key::Enter) {
                                    self.new_channel(
                                        &self.new_channel_name.clone(),
                                        (&self.def_filter).try_into().unwrap(),
                                    );
                                }
                            });
                        }
                        if new_channel_ui.button("+").clicked() {
                            self.new_channel(
                                &self.new_channel_name.clone(),
                                (&self.def_filter).try_into().unwrap(),
                            );
                        }
                    });
                    if let Some(err) = &self.error_msg {
                        channel_list_ui.label(RichText::new(err).color(Color32::RED));
                    }
                });
            });
            main_area_ui.separator();
            let available_width = main_area_ui.available_width() - 50.0;
            if !self.channel_list.is_empty() {
                self.draw_chat(
                    main_area_ui,
                    vec2(available_width / 2.0, main_area_available_size.y),
                    false,
                    self.font_size,
                );
                main_area_ui.separator();
                self.draw_chat(
                    main_area_ui,
                    vec2(available_width / 2.0, main_area_available_size.y),
                    true,
                    self.font_size,
                );
                self.channel_list[self.selected_channel].read();
            }
        });
        if let Some(idx) = remove_channel {
            self.channel_list.remove(idx);
            if self.selected_channel == idx {
                if idx > 0 {
                    self.selected_channel = idx - 1;
                }
            }
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

    pub fn channel_list_mut(&mut self) -> &mut [ChatClient] {
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
        self.channel_list = save_state
            .channels
            .iter()
            .map(|save| {
                let mut client = ChatClient::new(
                    save.name.clone(),
                    super::MAX_MESSAGE_COUNT,
                    (&save.filter).try_into()?,
                );
                if save.enabled {
                    client.connect().unwrap();
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
            .collect::<Result<Vec<ChatClient>, regex::Error>>()?;
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
        Ok(())
    }

    pub fn texture_map(&mut self) -> &mut HashMap<String, TextureHandle> {
        &mut self.textures
    }

    fn draw_chat(&mut self, ui: &mut Ui, size: Vec2, filtered: bool, font_size: f32) {
        ui.group(|group_ui| {
            group_ui.vertical(|ui| {
                ui.set_width(size.x);
                ui.set_height(size.y);
                ui.horizontal(|ui| {
                    ui.label(if filtered {
                        "Filtered message"
                    } else {
                        "All message"
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        let log_status = if filtered {
                            self.channel_list[self.selected_channel].filtered_log_status()
                        } else {
                            self.channel_list[self.selected_channel].log_status()
                        };
                        match log_status {
                            None => {
                                let response = ui.button("Log");
                                if self.channel_list[self.selected_channel].state()
                                    != ChannelConnectionState::Joined
                                {
                                    response.on_hover_text("Please enable the channel first.");
                                } else {
                                    if response.clicked() {
                                        self.log_btn = Some((self.selected_channel, filtered));
                                    }
                                }
                            }
                            Some(r) => match r {
                                Err(e) => {
                                    if ui
                                        .button(RichText::new("Stop").color(Color32::RED))
                                        .on_hover_text(e)
                                        .clicked()
                                    {
                                        if filtered {
                                            self.channel_list[self.selected_channel]
                                                .set_filtered_log(None);
                                        } else {
                                            self.channel_list[self.selected_channel].set_log(None);
                                        }
                                    }
                                }
                                Ok(p) => {
                                    if ui
                                        .button(RichText::new("Stop").color(Color32::GREEN))
                                        .on_hover_text(format!("{}", p.display()))
                                        .clicked()
                                    {
                                        if filtered {
                                            self.channel_list[self.selected_channel]
                                                .set_filtered_log(None);
                                        } else {
                                            self.channel_list[self.selected_channel].set_log(None);
                                        }
                                    }
                                }
                            },
                        }
                    });
                });
                ui.separator();
                let mut end_pressed = false;
                let mut home_pressed = false;

                ui.input_mut(|input| {
                    if input.focused {
                        if input.consume_key(Modifiers::default(), Key::End) {
                            end_pressed = true;
                        }
                        if input.consume_key(Modifiers::default(), Key::Home) {
                            home_pressed = true;
                        }
                    }
                });

                ScrollArea::vertical()
                    .id_source(filtered)
                    .auto_shrink([false; 2])
                    .stick_to_bottom(!home_pressed)
                    .show(ui, |ui| {
                        if home_pressed {
                            ui.scroll_to_cursor(None);
                        }
                        let client = &self.channel_list[self.selected_channel];
                        for msg in client.get_msg(filtered) {
                            self.draw_msg(ui, &msg, font_size);
                        }
                        if end_pressed {
                            ui.scroll_to_cursor(None);
                        }
                    })

                //area.show_rows(ui, row_height, num_rows, |ui, row_range| {
                //    let client = &self.channel_list[self.selected_channel];
                //    for msg in client.get_n_msg(row_range, filtered) {
                //        self.draw_msg(ui, &msg, font_size);
                //    }
                //});
            });
        });
    }

    fn draw_msg(&mut self, ui: &mut Ui, msg: &PrivmsgMessage, font_size: f32) {
        let text_style = TextStyle::Body;
        let row_height = ui.text_style_height(&text_style) + 1.0;
        let mut color = if self.use_twitch_color {
            msg.name_color
                .map(|c| Color32::from_rgb(c.r, c.g, c.b))
                .unwrap_or(ui.style().visuals.warn_fg_color)
        } else {
            ui.style().visuals.warn_fg_color
        };

        if self.readable_color_adjustment {
            color = adjust_readable_color(color, ui.visuals().panel_fill);
        }
        if let Some(((reply_author_id, reply_author_name), reply_msg_body)) = msg
            .source
            .tags
            .0
            .get("reply-parent-user-login")
            .map(|o| o.as_ref())
            .flatten()
            .zip(
                msg.source
                    .tags
                    .0
                    .get("reply-parent-display-name")
                    .map(|o| o.as_ref())
                    .flatten(),
            )
            .zip(
                msg.source
                    .tags
                    .0
                    .get("reply-parent-msg-body")
                    .map(|o| o.as_ref())
                    .flatten(),
            )
        {
            ui.horizontal(|ui| {
                ui.add(Label::new(
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
                ));
            });
        }
        ui.horizontal_wrapped(|ui| {
            if self.show_sent_time {
                let local_time = msg.server_timestamp.with_timezone(&chrono::Local);
                ui.label(local_time.format("%H:%M:%S").to_string());
            }
            for badge in msg.badges.iter() {
                if badge.name == super::filter::BROADCASTER_BADGE_NAME {
                    ui.image(
                        self.textures
                            .get(super::filter::BROADCASTER_BADGE_NAME)
                            .unwrap(),
                        vec2(row_height, row_height),
                    );
                }
                if badge.name == super::filter::MODERATOR_BADGE_NAME {
                    ui.image(
                        self.textures
                            .get(super::filter::MODERATOR_BADGE_NAME)
                            .unwrap(),
                        vec2(row_height, row_height),
                    );
                }
                if badge.name == super::filter::PARTNER_BADGE_NAME {
                    ui.image(
                        self.textures
                            .get(super::filter::PARTNER_BADGE_NAME)
                            .unwrap(),
                        vec2(row_height, row_height),
                    );
                }
                if badge.name == super::filter::VIP_BADGE_NAME {
                    ui.image(
                        self.textures.get(super::filter::VIP_BADGE_NAME).unwrap(),
                        vec2(row_height, row_height),
                    );
                }
            }
            let mut layout = LayoutJob {
                wrap: TextWrapping {
                    max_width: ui.available_width(),
                    break_anywhere: true,
                    ..Default::default()
                },
                ..Default::default()
            };
            let format: TextFormat = TextFormat {
                font_id: FontId::new(font_size, Proportional),
                color: ui.visuals().text_color(),
                ..Default::default()
            };
            let name = match self.name_display {
                NameDisplay::Both => format!("{}({})", msg.sender.name, msg.sender.login),
                NameDisplay::NickName => msg.sender.name.to_owned(),
                NameDisplay::Id => msg.sender.login.to_owned(),
            };
            layout.append(
                &name,
                0.0,
                TextFormat {
                    color,
                    ..format.clone()
                },
            );
            layout.append(": ", 0.0, format.clone());
            layout.append(msg.message_text.trim(), 0.0, format);

            let response: Response = ui.label(layout).context_menu(|ui| {
                ui.set_width(400.0);
                ui.hyperlink_to(
                    format!("{}({})", msg.sender.name, msg.sender.login),
                    format!("https://www.twitch.tv/{}", msg.sender.login),
                );
                ui.separator();
                let mut drawed_badge = false;
                for badge in msg.badges.iter() {
                    if badge.name == super::filter::BROADCASTER_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::BROADCASTER_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Broadcaster");
                        });
                        drawed_badge = true;
                    }
                    if badge.name == super::filter::MODERATOR_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::MODERATOR_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Moderator");
                        });
                        drawed_badge = true;
                    }
                    if badge.name == super::filter::PARTNER_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self
                                .textures
                                .get(super::filter::PARTNER_BADGE_NAME)
                                .unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("Partner");
                        });
                        drawed_badge = true;
                    }
                    if badge.name == super::filter::VIP_BADGE_NAME {
                        ui.horizontal(|ui| {
                            let texture = self.textures.get(super::filter::VIP_BADGE_NAME).unwrap();
                            ui.image(texture, vec2(row_height, row_height));
                            ui.label("VIP");
                        });
                        drawed_badge = true;
                    }
                }
                if drawed_badge {
                    ui.separator();
                }
                if ui.button("Add user to filter").clicked() {
                    match Regex::new(&regex::escape(&msg.sender.login)) {
                        Ok(r) => self.channel_list[self.selected_channel]
                            .mut_filter(|f| f.add_author_pat(r)),
                        Err(e) => self.error_msg = Some(format!("{}", e)),
                    }
                    dbg!("clicked");
                    ui.close_menu();
                }
                if ui.button("Add user to exclusive filter").clicked() {
                    match Regex::new(&regex::escape(&msg.sender.login)) {
                        Ok(r) => self.channel_list[self.selected_channel]
                            .mut_filter(|f| f.add_exc_author_pat(r)),
                        Err(e) => self.error_msg = Some(format!("{}", e)),
                    }
                    dbg!("clicked");
                    ui.close_menu();
                }
                if ui.button("Copy content").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(msg.message_text.trim()).unwrap();
                    dbg!("clicked");
                    ui.close_menu();
                }
                if ui.button("Copy sender's nickname").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(&msg.sender.name).unwrap();
                    dbg!("clicked");
                    ui.close_menu();
                }
                if ui.button("Copy sender's id").clicked() {
                    let mut clipboard = Clipboard::new().unwrap();
                    clipboard.set_text(&msg.sender.login).unwrap();
                    dbg!("clicked");
                    ui.close_menu();
                }
            });
            if response.hovered() {
                response.highlight();
            }
        });
        ui.separator();
    }
}

impl eframe::App for EguiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
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
        ctx.request_repaint_after(Duration::from_secs(1));
        egui::CentralPanel::default().show(ctx, |app_ui| {
            if app_ui
                .button(match self.state {
                    AppState::Normal => "Configuration",
                    AppState::Config => "Back",
                    AppState::ChannelConfig(_, _) => "Save",
                })
                .clicked()
            {
                match &self.state {
                    AppState::Normal => {
                        self.error_msg = None;
                        self.state = AppState::Config;
                    }
                    AppState::Config => {
                        if let Err(e) = build_regexes(&self.def_filter.inc_author) {
                            self.error_msg = Some(format!("{}", e));
                        } else if let Err(e) = build_regexes(&self.def_filter.inc_msg) {
                            self.error_msg = Some(format!("{}", e));
                        } else {
                            self.error_msg = None;
                            self.state = AppState::Normal;
                        }
                    }
                    AppState::ChannelConfig(idx, state) => {
                        match self.channel_list[*idx].set_filter(state) {
                            Ok(_) => {
                                self.error_msg = None;
                                self.state = AppState::Normal;
                            }
                            Err(e) => self.error_msg = Some(format!("{}", e)),
                        }
                    }
                }
            }
            app_ui.separator();
            match &mut self.state {
                AppState::Normal => self.draw_normal(app_ui),
                AppState::Config => self.draw_config(app_ui, ctx),
                AppState::ChannelConfig(_, _) => self.draw_channel_config(app_ui),
            }
        });
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
                    log_status: c.log_status().map(|r| r.ok()).flatten(),
                    filtered_log_status: c.filtered_log_status().map(|r| r.ok()).flatten(),
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
    let mut color = fg.clone();
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