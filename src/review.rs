use chrono::NaiveDateTime;
use serde::Deserialize;
use std::collections::HashMap;
use toml::value::Datetime;

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub project: String,
    pub branch: String,
    pub id: String,
    number: i32,
    pub owner: User,
    commit_message: String,
    pub comments: Vec<Comment>,
    pub patch_sets: Vec<PatchSet>,
}

#[derive(Debug, Deserialize, Default)]
pub struct User {
    pub name: String,
    pub username: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct Comment {
    pub reviewer: User,
    pub message: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct PatchSet {
    pub approvals: Option<Vec<Approval>>,
    pub comments: Option<Vec<Comment>>,
}

#[derive(Debug, Deserialize, Default)]
pub struct Approval {
    #[serde(rename = "type")]
    pub review_type: String,
    pub value: String,
    #[serde(rename = "grantedOn")]
    pub granted_on: i64,
    pub by: User,
}

trait Timestamp {
    fn timestamp(&self, time: &str) -> i64;
}

/// Extends toml::value::Datetime with a function that returns timestamp.
impl Timestamp for Datetime {
    fn timestamp(&self, time: &str) -> i64 {
        let datetime = format!("{}T{}", &self.to_string(), time);
        NaiveDateTime::parse_from_str(&datetime, "%Y-%m-%dT%H:%M:%S")
            .expect("Failed to parse datetime")
            .timestamp()
    }
}

impl Review {
    pub fn new(line: &str) -> Self {
        serde_json::from_str(line).expect("Failed to parse json")
    }

    pub fn is_within_date(&self, from: &Datetime, to: &Datetime) -> bool {
        let from = from.timestamp("00:00:00");
        let to = to.timestamp("23:59:59");
        let patch = self
            .patch_sets
            .last()
            .expect("Failed to get last patch set");

        for approval in patch
            .approvals
            .as_ref()
            .expect("Failed to get approval change")
        {
            if approval.review_type == "SUBM"
                && from <= approval.granted_on
                && approval.granted_on <= to
            {
                return true;
            }
        }

        false
    }

    pub fn repository_name(&self) -> String {
        self.project.to_string()
    }

    pub fn comments_made(&self, users: &HashMap<String, String>) -> HashMap<String, u32> {
        let mut user_comments: HashMap<String, u32> = HashMap::new();

        for patch in &self.patch_sets {
            if let Some(comments) = &patch.comments {
                for comment in comments {
                    if users.contains_key(&comment.reviewer.username)
                        && comment.reviewer.username != self.owner.username
                    {
                        *user_comments
                            .entry(comment.reviewer.username.to_string())
                            .or_insert(0) += 1;
                    }
                }
            }
        }

        user_comments
    }

    pub fn comments_received(&self) -> u32 {
        let mut received = 0u32;

        for patch in &self.patch_sets {
            if let Some(comments) = &patch.comments {
                received += comments.len() as u32;
            }
        }

        received
    }

    pub fn approvals(&self, users: &HashMap<String, String>) -> Vec<String> {
        let mut approval_users = Vec::new();
        let patch = self
            .patch_sets
            .last()
            .expect("Failed to get last patch set");

        for approval in patch
            .approvals
            .as_ref()
            .expect("Failed to get approval change")
        {
            if approval.review_type == "Code-Review"
                && approval.value == "2"
                && users.contains_key(&approval.by.username)
            {
                approval_users.push(approval.by.username.clone());
            }
        }

        approval_users
    }

    pub fn patch_set_count(&self) -> u32 {
        self.patch_sets.len() as u32
    }

    pub fn commit_message_words(&self) -> u32 {
        self.commit_message.split_whitespace().count() as u32
    }
}
