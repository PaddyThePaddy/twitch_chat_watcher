use crate::audio_player::AlertPlayer;

use super::filter::{Filter, FilterState};
use std::io::Write;
use std::{
    collections::VecDeque,
    fs::OpenOptions,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use tokio::task::JoinHandle;
use twitch_irc::{
    login::StaticLoginCredentials,
    message::{PrivmsgMessage, ServerMessage},
    validate, ClientConfig, SecureTCPTransport, TwitchIRCClient,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ChannelConnectionState {
    Uninitialized,
    Joined,
}

struct SharedData {
    msg_list: VecDeque<PrivmsgMessage>,
    filtered_msg_list: VecDeque<PrivmsgMessage>,
    filter: Filter,
    max_msg_count: usize,
    state: ChannelConnectionState,
    log: Option<Result<PathBuf, std::io::Error>>,
    log_filtered: Option<Result<PathBuf, std::io::Error>>,
    alert: Option<AlertPlayer>,
    has_unread_filtered_msg: bool,
}

pub struct ChatClient {
    channel_name: String,
    shared_data: Arc<Mutex<SharedData>>,
    connected: bool,
    _client: Option<TwitchIRCClient<SecureTCPTransport, StaticLoginCredentials>>,
    _worker_handle: Option<JoinHandle<()>>,
}

impl ChatClient {
    pub fn new(channel: impl ToString, max_msg_count: usize, filter: Filter) -> ChatClient {
        let shared_data = Arc::new(Mutex::new(SharedData {
            msg_list: VecDeque::new(),
            filtered_msg_list: VecDeque::new(),
            filter,
            max_msg_count,
            state: ChannelConnectionState::Uninitialized,
            log: None,
            log_filtered: None,
            alert: None,
            has_unread_filtered_msg: false,
        }));
        Self {
            channel_name: channel.to_string().to_lowercase(),
            shared_data,
            _client: None,
            _worker_handle: None,
            connected: false,
        }
    }

    pub fn connect(&mut self) -> Result<(), validate::Error> {
        let irc_config: ClientConfig<StaticLoginCredentials> = ClientConfig::default();
        let (mut incoming_messages, client) =
            TwitchIRCClient::<SecureTCPTransport, StaticLoginCredentials>::new(irc_config);
        let worker_shared_data = self.shared_data.clone();

        let join_handle = tokio::spawn(async move {
            while let Some(message) = incoming_messages.recv().await {
                match message {
                    ServerMessage::Privmsg(mut msg) => {
                        eprintln!("{:?}", msg);
                        if msg.message_text.ends_with('\u{e0000}') {
                            msg.message_text =
                                msg.message_text.trim_end_matches('\u{e0000}').to_owned();
                        }
                        let mut shared_data: std::sync::MutexGuard<'_, SharedData> =
                            worker_shared_data.lock().unwrap();
                        shared_data.msg_list.push_back(msg.clone());
                        if let Some(path) = &shared_data.log {
                            if let Ok(p) = path {
                                match OpenOptions::new().create(true).append(true).open(p) {
                                    Ok(mut f) => {
                                        f.write(msg_to_str(&msg).as_bytes()).unwrap();
                                    }
                                    Err(e) => shared_data.log = Some(Err(e)),
                                }
                            }
                        }
                        while shared_data.msg_list.len() > shared_data.max_msg_count {
                            shared_data.msg_list.pop_front();
                        }
                        if shared_data.filter.test(&msg) {
                            if let Some(path) = &shared_data.log_filtered {
                                if let Ok(p) = path {
                                    match OpenOptions::new().create(true).append(true).open(p) {
                                        Ok(mut f) => {
                                            f.write(msg_to_str(&msg).as_bytes()).unwrap();
                                        }
                                        Err(e) => shared_data.log_filtered = Some(Err(e)),
                                    }
                                }
                            }
                            shared_data.filtered_msg_list.push_back(msg);
                            while shared_data.filtered_msg_list.len() > shared_data.max_msg_count {
                                shared_data.filtered_msg_list.pop_front();
                            }
                            if let Some(player) = &shared_data.alert {
                                player.play().unwrap();
                            }
                            shared_data.has_unread_filtered_msg = true;
                        }
                    }
                    ServerMessage::Join(_) => {
                        worker_shared_data.lock().unwrap().state = ChannelConnectionState::Joined;
                    }
                    _ => {
                        //eprintln!("{:?}", message);
                    }
                }
            }
        });
        client.join(self.channel_name.clone())?;
        self._worker_handle = Some(join_handle);
        self._client = Some(client);
        self.connected = true;
        Ok(())
    }

    pub fn disconnect(&mut self) {
        self._worker_handle = None;
        self._client = None;
        self.shared_data.lock().unwrap().state = ChannelConnectionState::Uninitialized;
        self.connected = false;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn get_n_msg(&self, range: std::ops::Range<usize>, filtered: bool) -> Vec<PrivmsgMessage> {
        let mut result = vec![];
        let lock = self.shared_data.lock().unwrap();
        let msg_list = if filtered {
            &lock.filtered_msg_list
        } else {
            &lock.msg_list
        };
        for i in range {
            result.push(msg_list[i].clone());
        }
        result
    }

    pub fn get_msg(&self, filtered: bool) -> Vec<PrivmsgMessage> {
        let mut result = vec![];
        let lock = self.shared_data.lock().unwrap();
        let msg_list = if filtered {
            &lock.filtered_msg_list
        } else {
            &lock.msg_list
        };
        for msg in msg_list.iter() {
            result.push(msg.clone());
        }
        result
    }

    pub fn get_msg_count(&self, filtered: bool) -> usize {
        if filtered {
            self.shared_data.lock().unwrap().filtered_msg_list.len()
        } else {
            self.shared_data.lock().unwrap().msg_list.len()
        }
    }

    pub fn state(&self) -> ChannelConnectionState {
        self.shared_data.lock().unwrap().state
    }

    pub fn mut_filter<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Filter),
    {
        op(&mut self.shared_data.lock().unwrap().filter);
    }

    pub fn get_filter_state(&self) -> FilterState {
        (&self.shared_data.lock().unwrap().filter).into()
    }

    pub fn set_filter(&mut self, state: &FilterState) -> Result<(), regex::Error> {
        self.shared_data.lock().unwrap().filter = state.try_into()?;
        Ok(())
    }

    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    pub fn set_log(&mut self, p: Option<PathBuf>) {
        self.shared_data.lock().unwrap().log = p.map(|p| Ok(p));
    }

    pub fn log_status(&self) -> Option<Result<PathBuf, String>> {
        self.shared_data
            .lock()
            .unwrap()
            .log
            .as_ref()
            .map(|r| r.as_ref().map_err(|e| e.to_string()).map(|p| p.clone()))
    }

    pub fn set_filtered_log(&mut self, p: Option<PathBuf>) {
        self.shared_data.lock().unwrap().log_filtered = p.map(|p| Ok(p));
    }

    pub fn filtered_log_status(&self) -> Option<Result<PathBuf, String>> {
        self.shared_data
            .lock()
            .unwrap()
            .log_filtered
            .as_ref()
            .map(|r| r.as_ref().map_err(|e| e.to_string()).map(|p| p.clone()))
    }

    pub fn alert(&self) -> bool {
        self.shared_data.lock().unwrap().alert.is_some()
    }

    pub fn set_alert(&mut self, alert: Option<AlertPlayer>) {
        self.shared_data.lock().unwrap().alert = alert;
    }

    pub fn has_unread_filtered_msg(&self) -> bool {
        self.shared_data.lock().unwrap().has_unread_filtered_msg
    }

    pub fn read(&mut self) {
        self.shared_data.lock().unwrap().has_unread_filtered_msg = false;
    }
}

fn msg_to_str(msg: &PrivmsgMessage) -> String {
    format!(
        "{} {}({}): {}\n",
        msg.server_timestamp
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S"),
        msg.sender.name,
        msg.sender.login,
        msg.message_text
    )
}
