use regex::Regex;
use serde::{Deserialize, Serialize};
use twitch_irc::message::PrivmsgMessage;

pub const BROADCASTER_BADGE_NAME: &str = "broadcaster";
pub const MODERATOR_BADGE_NAME: &str = "moderator";
pub const VIP_BADGE_NAME: &str = "vip";
pub const PARTNER_BADGE_NAME: &str = "partner";

#[derive(Clone, Deserialize, Serialize, Default)]
pub struct FilterState {
    pub inc_msg: String,
    pub inc_author: String,
    pub exc_msg: String,
    pub exc_author: String,
    pub broadcaster: bool,
    pub moderator: bool,
    pub vip: bool,
    pub partner: bool,
}

impl std::convert::From<&Filter> for FilterState {
    fn from(value: &Filter) -> Self {
        let msg_vec: Vec<&str> = value.inc_msg_pat.iter().map(|r| r.as_str()).collect();
        let author_vec: Vec<&str> = value.inc_author_pat.iter().map(|r| r.as_str()).collect();
        let exc_msg_vec: Vec<&str> = value.exc_msg_pat.iter().map(|r| r.as_str()).collect();
        let exc_author_vec: Vec<&str> = value.exc_author_pat.iter().map(|r| r.as_str()).collect();
        Self {
            inc_msg: msg_vec.join("\n"),
            inc_author: author_vec.join("\n"),
            exc_msg: exc_msg_vec.join("\n"),
            exc_author: exc_author_vec.join("\n"),
            broadcaster: value
                .badge_pat
                .contains(&BROADCASTER_BADGE_NAME.to_string()),
            moderator: value.badge_pat.contains(&MODERATOR_BADGE_NAME.to_string()),
            vip: value.badge_pat.contains(&VIP_BADGE_NAME.to_string()),
            partner: value.badge_pat.contains(&PARTNER_BADGE_NAME.to_string()),
        }
    }
}

#[derive(Clone, Default)]
pub struct Filter {
    inc_msg_pat: Vec<Regex>,
    inc_author_pat: Vec<Regex>,
    badge_pat: Vec<String>,
    exc_msg_pat: Vec<Regex>,
    exc_author_pat: Vec<Regex>,
}

impl TryFrom<&FilterState> for Filter {
    type Error = regex::Error;
    fn try_from(value: &FilterState) -> Result<Self, Self::Error> {
        let mut msg = vec![];
        let mut author = vec![];
        let mut exc_msg = vec![];
        let mut exc_author = vec![];
        let mut badge_pat = vec![];

        for line in value
            .inc_msg
            .split('\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            msg.push(Regex::new(line)?);
        }
        for line in value
            .inc_author
            .split('\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            author.push(Regex::new(line)?);
        }
        for line in value
            .exc_msg
            .split('\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            exc_msg.push(Regex::new(line)?);
        }
        for line in value
            .exc_author
            .split('\n')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            exc_author.push(Regex::new(line)?);
        }
        if value.broadcaster {
            badge_pat.push(BROADCASTER_BADGE_NAME.to_owned())
        }
        if value.moderator {
            badge_pat.push(MODERATOR_BADGE_NAME.to_owned())
        }
        if value.vip {
            badge_pat.push(VIP_BADGE_NAME.to_owned())
        }
        if value.partner {
            badge_pat.push(PARTNER_BADGE_NAME.to_owned())
        }

        Ok(Self {
            inc_msg_pat: msg,
            inc_author_pat: author,
            badge_pat,
            exc_msg_pat: exc_msg,
            exc_author_pat: exc_author,
        })
    }
}

impl Filter {
    pub fn set_msg_pat(&mut self, pat: Vec<Regex>) {
        self.inc_msg_pat = pat;
    }

    pub fn set_author_pat(&mut self, pat: Vec<Regex>) {
        self.inc_author_pat = pat;
    }

    pub fn set_badge_pat(&mut self, pat: Vec<String>) {
        self.badge_pat = pat;
    }

    pub fn add_author_pat(&mut self, pat: Regex) {
        self.inc_author_pat.push(pat);
    }

    pub fn add_exc_author_pat(&mut self, pat: Regex) {
        self.exc_author_pat.push(pat);
    }

    pub fn test(&self, msg: &PrivmsgMessage) -> bool {
        for pat in self.exc_msg_pat.iter() {
            if pat.is_match(&msg.message_text) {
                return false;
            }
        }

        for pat in self.exc_author_pat.iter() {
            if pat.is_match(&msg.sender.login) {
                return false;
            }
        }

        for pat in self.inc_msg_pat.iter() {
            if pat.is_match(&msg.message_text) {
                return true;
            }
        }

        for pat in self.inc_author_pat.iter() {
            if pat.is_match(&msg.sender.login) {
                return true;
            }
        }
        for badge in msg.badges.iter() {
            for target in self.badge_pat.iter() {
                if &badge.name == target {
                    return true;
                }
            }
        }
        false
    }
}
