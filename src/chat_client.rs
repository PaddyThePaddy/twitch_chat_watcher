use super::{
    audio_player::AlertPlayer,
    filter::{Filter, FilterState},
    ASYNC_RUNTIME,
};
use chrono::{DateTime, TimeZone, Utc};
use futures::stream::StreamExt;
use irc::{
    client::{
        prelude::{Command, Config},
        Client, ClientStream,
    },
    proto::{
        message::Tag,
        {Capability, Message, Prefix, Response},
    },
};
use std::{
    collections::{HashMap, VecDeque},
    path::PathBuf,
    sync::Arc,
};
use tokio::{
    fs::OpenOptions,
    io::AsyncWriteExt,
    sync::{mpsc, Mutex},
    task::JoinHandle,
};

pub struct IrcClient {
    new_channel_tx: mpsc::Sender<(String, Option<Arc<Mutex<SharedData>>>)>,
    send_msg_tx: mpsc::Sender<(String, String)>,
    _worker_handle: JoinHandle<()>,
}

impl IrcClient {
    pub async fn new(username: impl ToString, password: impl ToString) -> Result<Self, ()> {
        let (new_channel_tx, mut new_channel_rx) =
            mpsc::channel::<(String, Option<Arc<Mutex<SharedData>>>)>(10);
        let mut worker_username = username.to_string();
        let password = password.to_string();
        let (mut client, mut stream) = connect_client(&worker_username, &password).await.unwrap();
        client
            .send_cap_req(&[
                Capability::Custom("twitch.tv/commands"),
                Capability::Custom("twitch.tv/tags"),
                Capability::EchoMessage,
            ])
            .unwrap();
        let first_msg = stream.next().await.and_then(|r| r.ok());
        if let Some(msg) = first_msg {
            if let Command::Response(t, message) = msg.command {
                if !(t == Response::RPL_WELCOME) {
                    return Err(());
                } else {
                    worker_username = message[0].clone();
                }
            } else {
                return Err(());
            }
        } else {
            return Err(());
        }
        let (msg_tx, mut msg_rx) = mpsc::channel::<(String, String)>(10);
        //eprintln!("starting");
        let handle = ASYNC_RUNTIME.spawn(async move {
            //dbg!("starting worker");
            let mut channel_dict: HashMap<String, Arc<Mutex<SharedData>>> = HashMap::new();
            let mut sent_msg = None;
            loop {
                tokio::select! {
                    Some((target, msg)) = msg_rx.recv() => {
                        client.send_privmsg(&target, &msg).unwrap();
                        sent_msg = Some(msg.clone());
                    }
                    Some((channel_name, data_opt)) = new_channel_rx.recv() => {
                        if let Some(data) = data_opt {
                            let channel_name = format!("#{}", channel_name);
                            //eprintln!("Joining {}", channel_name);
                            client.send_join(&channel_name).unwrap();
                            channel_dict.insert(channel_name, data);
                        } else {
                            //eprintln!("Parting {}", channel_name);
                            let channel_name = format!("#{}", channel_name);
                            client.send_part(&channel_name).unwrap();
                            if let Some(data) = channel_dict.remove(&channel_name) {
                                data.lock().await.state = ChannelConnectionState::Uninitialized;
                            }
                        }
                    }
                    Some(msg) = stream.next() => {
                        match msg {
                            Err(_) => {
                                //eprintln!("{:?}", err);
                                (client, stream) = connect_client(&worker_username, &password).await.unwrap();
                                for channel in channel_dict.keys() {
                                    client.send_join(channel).unwrap();
                                }
                            }
                            Ok(msg) => {
                            //eprintln!("{:?}", msg);
                            match &msg.command {
                                Command::JOIN(channel_list, _channel_keys, _real_name) => {
                                    if let Some(data) = channel_dict.get(channel_list) {
                                        data.lock().await.state = ChannelConnectionState::Joined;
                                    if let Ok(tw_msg) = TwitchMsg::try_from(msg.clone()){
                                        handle_msg(tw_msg, data).await;
                                    }
                                    }
                                }
                                Command::PRIVMSG(target, _payload) => {
                                    if let Ok(tw_msg) = TwitchMsg::try_from(msg.clone()){
                                    if let Some(data) = channel_dict.get(target) {
                                        handle_msg(tw_msg, data).await;
                                    }
                                }
                                }
                                Command::NOTICE(_, content) => {
                                    if content == "Login authentication failed" {
                                        panic!()
                                    }
                                }
                                Command::Raw(t, channel_list) => {
                                    if t == "USERSTATE" {
                                        if let Some(sent_msg_content) = sent_msg {
                                            let mut tags = msg.tags.clone().unwrap();
                                            tags.push(Tag("tmi-sent-ts".to_string(), Some(format!("{}", chrono::Utc::now().timestamp_millis()))));
                                            let prefix = format!("{}!{}@{}.tmi.twitch.tv", &worker_username, &worker_username, &worker_username);
                                            let msg_obj = Message::with_tags(Some(tags), Some(&prefix), "PRIVMSG", vec![&channel_list[0], &sent_msg_content]).unwrap();
                                            if let Some(data) = channel_dict.get(&channel_list[0]) {
                                                handle_msg(TwitchMsg::try_from(msg_obj).unwrap(), data).await;
                                            }
                                            sent_msg = None;
                                        }

                                    }
                                }
                                _ => {}
                            }
                        },
                    }
                }
                }
            }
        });

        Ok(Self {
            new_channel_tx,
            _worker_handle: handle,
            send_msg_tx: msg_tx,
        })
    }

    async fn join(&mut self, channel_name: impl ToString, data: Arc<Mutex<SharedData>>) {
        //eprintln!("Join");
        let channel_name = channel_name.to_string();
        self.new_channel_tx
            .send((channel_name.to_string(), Some(data)))
            .await
            .unwrap();
    }

    async fn part_channel(&self, channel_name: impl ToString) {
        let channel_name = channel_name.to_string();
        self.new_channel_tx
            .send((channel_name.to_string(), None))
            .await
            .unwrap();
    }

    async fn send_msg(&self, target: String, msg: String) {
        self.send_msg_tx.send((target, msg)).await.unwrap()
    }
}

impl Drop for IrcClient {
    fn drop(&mut self) {
        self._worker_handle.abort();
    }
}

async fn handle_msg(mut msg: TwitchMsg, data: &Arc<Mutex<SharedData>>) {
    if msg.payload.ends_with('\u{e0000}') {
        msg.payload = msg.payload.trim_end_matches('\u{e0000}').to_owned();
    }
    let mut shared_data = data.lock().await;
    shared_data.msg_list.push_back(msg.clone());
    if let Some(Ok(p)) = &shared_data.log {
        match OpenOptions::new().create(true).append(true).open(p).await {
            Ok(mut f) => f.write_all(msg_to_str(&msg).as_bytes()).await.unwrap(),
            Err(e) => shared_data.log = Some(Err(e)),
        }
    }
    while shared_data.msg_list.len() > shared_data.max_msg_count {
        shared_data.msg_list.pop_front();
    }
    if shared_data.filter.test(&msg) {
        if let Some(Ok(p)) = &shared_data.log_filtered {
            match OpenOptions::new().create(true).append(true).open(p).await {
                Ok(mut f) => f.write_all(msg_to_str(&msg).as_bytes()).await.unwrap(),
                Err(e) => shared_data.log = Some(Err(e)),
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

async fn connect_client(
    username: &str,
    password: &str,
) -> Result<(Client, ClientStream), irc::error::Error> {
    let irc_config = Config {
        nickname: Some(username.to_string()),
        server: Some("irc.chat.twitch.tv".to_string()),
        channels: vec![],
        password: Some(password.to_string()),
        port: Some(6667),
        use_tls: Some(false),
        ping_timeout: Some(10),
        ping_time: Some(10),
        ..Default::default()
    };
    let mut client = Client::from_config(irc_config.clone()).await?;

    client.identify()?;

    let stream = client.stream()?;
    Ok((client, stream))
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ChannelConnectionState {
    Uninitialized,
    Joined,
}

#[derive(Debug)]
struct SharedData {
    msg_list: VecDeque<TwitchMsg>,
    filtered_msg_list: VecDeque<TwitchMsg>,
    filter: Filter,
    max_msg_count: usize,
    state: ChannelConnectionState,
    log: Option<Result<PathBuf, std::io::Error>>,
    log_filtered: Option<Result<PathBuf, std::io::Error>>,
    alert: Option<AlertPlayer>,
    has_unread_filtered_msg: bool,
}

pub struct ChannelManager {
    channel_name: String,
    shared_data: Arc<Mutex<SharedData>>,
    connected: bool,
    client: Arc<tokio::sync::Mutex<IrcClient>>,
}

impl ChannelManager {
    pub fn new(
        client: Arc<tokio::sync::Mutex<IrcClient>>,
        channel: impl ToString,
        max_msg_count: usize,
        filter: Filter,
    ) -> ChannelManager {
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
            client,
            connected: false,
        }
    }

    pub fn connect(&mut self) {
        //dbg!("connecting");
        ASYNC_RUNTIME.block_on(async {
            self.client
                .lock()
                .await
                .join(&self.channel_name, self.shared_data.clone())
                .await
        });
        self.connected = true;
    }

    pub fn disconnect(&mut self) {
        ASYNC_RUNTIME.block_on(async {
            self.client
                .lock()
                .await
                .part_channel(&self.channel_name)
                .await
        });
        self.connected = false;
    }

    pub fn is_connected(&self) -> bool {
        self.connected
    }

    pub fn get_n_msg(&self, range: std::ops::Range<usize>, filtered: bool) -> Vec<TwitchMsg> {
        ASYNC_RUNTIME.block_on(async {
            let mut result = vec![];
            let lock = self.shared_data.lock().await;
            let msg_list = if filtered {
                &lock.filtered_msg_list
            } else {
                &lock.msg_list
            };
            for i in range {
                result.push(msg_list[i].clone());
            }
            result
        })
    }

    pub fn get_msg(&self, filtered: bool) -> Vec<TwitchMsg> {
        ASYNC_RUNTIME.block_on(async {
            let mut result = vec![];
            let lock = self.shared_data.lock().await;
            let msg_list = if filtered {
                &lock.filtered_msg_list
            } else {
                &lock.msg_list
            };
            for msg in msg_list.iter() {
                result.push(msg.clone());
            }
            result
        })
    }

    pub fn get_msg_count(&self, filtered: bool) -> usize {
        ASYNC_RUNTIME.block_on(async {
            if filtered {
                self.shared_data.lock().await.filtered_msg_list.len()
            } else {
                self.shared_data.lock().await.msg_list.len()
            }
        })
    }

    pub fn state(&self) -> ChannelConnectionState {
        ASYNC_RUNTIME.block_on(async { self.shared_data.lock().await.state })
    }

    pub fn mut_filter<F>(&mut self, op: F)
    where
        F: FnOnce(&mut Filter),
    {
        ASYNC_RUNTIME.block_on(async {
            op(&mut self.shared_data.lock().await.filter);
        })
    }

    pub fn get_filter_state(&self) -> FilterState {
        ASYNC_RUNTIME.block_on(async { (&self.shared_data.lock().await.filter).into() })
    }

    pub fn set_filter(&mut self, state: &FilterState) -> Result<(), regex::Error> {
        ASYNC_RUNTIME.block_on(async {
            match state.try_into() {
                Ok(f) => self.shared_data.lock().await.filter = f,
                Err(e) => return Err(e),
            }
            Ok(())
        })
    }

    pub fn channel_name(&self) -> &str {
        &self.channel_name
    }

    pub fn set_log(&mut self, p: Option<PathBuf>) {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data.lock().await.log = p.map(|p| Ok(p));
        });
    }

    pub fn log_status(&self) -> Option<Result<PathBuf, String>> {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data
                .lock()
                .await
                .log
                .as_ref()
                .map(|r: &Result<PathBuf, std::io::Error>| {
                    r.as_ref().map_err(|e| e.to_string()).map(|p| p.clone())
                })
        })
    }

    pub fn set_filtered_log(&mut self, p: Option<PathBuf>) {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data.lock().await.log_filtered = p.map(|p| Ok(p));
        });
    }

    pub fn filtered_log_status(&self) -> Option<Result<PathBuf, String>> {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data
                .lock()
                .await
                .log_filtered
                .as_ref()
                .map(|r| r.as_ref().map_err(|e| e.to_string()).map(|p| p.clone()))
        })
    }

    pub fn alert(&self) -> bool {
        ASYNC_RUNTIME.block_on(async { self.shared_data.lock().await.alert.is_some() })
    }

    pub fn set_alert(&mut self, alert: Option<AlertPlayer>) {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data.lock().await.alert = alert;
        });
    }

    pub fn has_unread_filtered_msg(&self) -> bool {
        ASYNC_RUNTIME.block_on(async { self.shared_data.lock().await.has_unread_filtered_msg })
    }

    pub fn read(&mut self) {
        ASYNC_RUNTIME.block_on(async {
            self.shared_data.lock().await.has_unread_filtered_msg = false;
        });
    }

    pub fn send_msg(&self, msg: String) {
        ASYNC_RUNTIME.block_on(async {
            self.client
                .lock()
                .await
                .send_msg(format!("#{}", self.channel_name), msg)
                .await;
        });
    }
}

fn msg_to_str(msg: &TwitchMsg) -> String {
    format!(
        "{} {}({}): {}\n",
        msg.sent_time()
            .unwrap()
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S"),
        msg.sender_display(),
        msg.sender_login(),
        msg.payload()
    )
}

#[derive(Debug, Clone)]
pub struct TwitchMsg {
    _source: Message,
    payload: String,
    sender_login: String,
    sender_display: String,
    channel: String,
    id: String,
}

impl TwitchMsg {
    pub fn payload(&self) -> &str {
        &self.payload
    }

    pub fn sender_login(&self) -> &str {
        &self.sender_login
    }

    pub fn sender_display(&self) -> &str {
        &self.sender_display
    }

    pub fn channel(&self) -> &str {
        &self.channel
    }

    pub fn tag(&self, tag_name: &str) -> Option<&String> {
        if let Some(tags) = &self._source.tags {
            search_tag(tag_name, tags)
        } else {
            None
        }
    }

    pub fn badges(&self) -> Vec<(String, String)> {
        let mut v = vec![];
        if let Some(badges_str) = self.tag("badges") {
            for badge_str in badges_str.split(',') {
                let mut splitter = badge_str.split('/');
                if let Some((badge_name, badge_ver)) = splitter.next().zip(splitter.next()) {
                    v.push((badge_name.to_string(), badge_ver.to_string()));
                }
            }
        }
        v
    }

    pub fn has_badge<'a>(&'a self, target: &str) -> Option<&'a str> {
        if let Some(badges_str) = self.tag("badges") {
            for badge_str in badges_str.split(',') {
                let mut splitter = badge_str.split('/');
                if let Some((badge_name, badge_ver)) = splitter.next().zip(splitter.next()) {
                    if badge_name == target {
                        return Some(badge_ver);
                    }
                }
            }
        }
        None
    }

    pub fn sent_time(&self) -> Option<DateTime<Utc>> {
        self.tag("tmi-sent-ts").map(|ts| {
            chrono::Utc
                .timestamp_millis_opt(ts.trim().parse::<i64>().unwrap())
                .unwrap()
        })
    }

    pub fn name_color(&self) -> Option<[u8; 3]> {
        if let Some(hex_str) = self.tag("color") {
            if hex_str.len() < 7 {
                return None;
            }
            u8::from_str_radix(&hex_str[1..3], 16)
                .ok()
                .zip(u8::from_str_radix(&hex_str[3..5], 16).ok())
                .zip(u8::from_str_radix(&hex_str[5..7], 16).ok())
                .map(|((r, g), b)| [r, g, b])
        } else {
            None
        }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
}

impl TryFrom<Message> for TwitchMsg {
    type Error = ();
    fn try_from(value: Message) -> Result<Self, Self::Error> {
        let (channel, payload) = if let Command::PRIVMSG(channel, payload) = &value.command {
            (channel.clone(), payload.clone())
        } else {
            return Err(());
        };
        let sender_login = if let Some(Prefix::Nickname(_, username, _)) = &value.prefix {
            username.clone()
        } else {
            return Err(());
        };
        let sender_display = if let Some(tags) = &value.tags {
            if let Some(s) = search_tag("display-name", tags) {
                s.clone()
            } else {
                return Err(());
            }
        } else {
            return Err(());
        };
        let id = if let Some(tags) = &value.tags {
            if let Some(s) = search_tag("id", tags) {
                s.clone()
            } else {
                return Err(());
            }
        } else {
            return Err(());
        };
        Ok(Self {
            _source: value,
            payload,
            sender_login,
            sender_display,
            channel,
            id,
        })
    }
}

fn search_tag<'a>(target: &str, tags: &'a [Tag]) -> Option<&'a String> {
    for tag in tags.iter() {
        if target == tag.0 {
            return tag.1.as_ref();
        }
    }
    None
}
