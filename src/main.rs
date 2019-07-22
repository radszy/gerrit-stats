mod review;

use crate::review::Review;
use clap::{App, Arg};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::collections::HashMap;
use toml::value::Datetime;

use futures::future::join_all;
use std::process::Command;
use tokio_core::reactor::Core;
use tokio_process::CommandExt;

#[derive(Debug, Deserialize)]
struct Config {
    server: String,
    port: String,
    from: Datetime,
    to: Datetime,
    user: Vec<User>,
}

impl Config {
    pub fn from_file(file_path: &str) -> Self {
        let config_str = std::fs::read_to_string(file_path).expect("Failed to read config file");

        let mut config: Config =
            toml::from_str(config_str.as_str()).expect("Failed to parse config file");
        config.fill_missing_dates();
        config
    }

    pub fn fill_missing_dates(&mut self) {
        for user in &mut self.user {
            if user.from.is_none() {
                user.from = Some(self.from.clone());
            }
            if user.to.is_none() {
                user.to = Some(self.to.clone());
            }
        }
    }

    fn user_dates(&self) -> HashMap<String, (Datetime, Datetime)> {
        let mut users: HashMap<String, (Datetime, Datetime)> = HashMap::new();
        for user in &self.user {
            users.insert(
                user.username.clone(),
                (user.from.clone().unwrap(), user.to.clone().unwrap()),
            );
        }
        users
    }

    fn user_names(&self) -> HashMap<String, String> {
        let mut users: HashMap<String, String> = HashMap::new();
        for user in &self.user {
            users.insert(user.username.clone(), user.fullname.clone());
        }
        users
    }
}

#[derive(Debug, Deserialize)]
struct User {
    username: String,
    fullname: String,
    from: Option<Datetime>,
    to: Option<Datetime>,
}

type UserStatistics = BTreeMap<String, BTreeMap<String, Stats>>;

#[derive(Debug, Default)]
struct Stats {
    changes: u32,
    approvals: u32,
    comments_made: u32,
    comments_received: u32,
    commit_words: u32,
    patch_sets: u32,
}

impl Stats {
    fn new() -> Self {
        Self {
            ..Default::default()
        }
    }
}

fn main() {
    let matches = App::new("gerrit-stats")
        .version("0.1.0")
        .author("Radek Szymanski <radszy@pm.me>")
        .about("\nGathers basic statistics based on the reviews users participated in.")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .value_name("FILE")
                .help("Path to a config file")
                .takes_value(true)
                .required(true),
        )
        .arg(
            Arg::with_name("user")
                .short("u")
                .long("user")
                .value_name("NAME")
                .help("Username for fetching Gerrit changes")
                .takes_value(true)
                .required(true),
        )
        .get_matches();

    let config_file = matches
        .value_of("config")
        .expect("Failed to read config option");

    let config = Config::from_file(config_file);

    let cmd_user = matches
        .value_of("user")
        .expect("Failed to read user option");

    let cmd_args = [
        "-p",
        config.port.as_str(),
        &format!("{}@{}", cmd_user, config.server),
        "gerrit",
        "query",
    ];

    let cmd_opts = [
        "--all-approvals",
        "--all-reviewers",
        "--comments",
        "--commit-message",
        "--files",
        "--format",
        "JSON",
    ];

    let mut cmds = Vec::new();

    println!("Spawning {} async tasks.", config.user.len());

    for user in &config.user {
        let child = Command::new("ssh")
            .stdout(std::process::Stdio::piped())
            .args(&cmd_args)
            .args(&cmd_opts)
            .arg("status:merged")
            .arg(format!("after:{}", user.from.clone().unwrap()))
            .arg(format!("before:{}", user.to.clone().unwrap()))
            .arg(format!("owner:{}", user.username))
            .spawn_async()
            .expect("Failed to spawn command")
            .wait_with_output();

        cmds.push(child);
    }

    println!("Starting work. This might take a while.");

    let work = join_all(cmds);
    let mut core = Core::new().expect("Failed to create reactor");
    let ret = core.run(work).expect("Failed to run work");

    let mut reviews = Vec::new();

    for output in &ret {
        let output = std::str::from_utf8(&output.stdout).expect("Failed to read command output");
        for line in output.lines().rev().skip(1) {
            let rev = Review::new(line);
            reviews.push(rev);
        }
    }

    let stats = collect_stats(&reviews, &config);
    write_simple_stats(&stats, &config);
    write_detailed_stats(&stats, &config);
}

fn collect_stats(reviews: &[Review], config: &Config) -> UserStatistics {
    fn add_stats(
        stats: &mut BTreeMap<String, Stats>,
        repo: String,
        received: u32,
        patches: u32,
        words: u32,
    ) {
        let total_stats = stats.entry(repo).or_insert_with(Stats::new);
        total_stats.changes += 1;
        total_stats.comments_received += received;
        total_stats.patch_sets += patches;
        total_stats.commit_words += words;
    }

    let dates = config.user_dates();
    let users = config.user_names();
    let mut stats: UserStatistics = BTreeMap::new();

    for review in reviews {
        if !review.is_within_date(
            &dates[&review.owner.username].0,
            &dates[&review.owner.username].1,
        ) {
            continue;
        }

        let repo = review.repository_name();
        let made = review.comments_made(&users);
        let received = review.comments_received();
        let approvals = review.approvals(&users);
        let patch_sets = review.patch_set_count();
        let words = review.commit_message_words();

        let user_stats = stats
            .entry(review.owner.username.to_string())
            .or_insert_with(Default::default);

        add_stats(user_stats, "All".to_string(), received, patch_sets, words);
        add_stats(user_stats, repo.to_string(), received, patch_sets, words);

        for (user, comment_count) in &made {
            let user_stats = stats
                .entry(user.to_string())
                .or_insert_with(Default::default);

            let total_stats = user_stats
                .entry("All".to_string())
                .or_insert_with(Stats::new);
            total_stats.comments_made += comment_count;

            let repo_stats = user_stats
                .entry(repo.to_string())
                .or_insert_with(Stats::new);
            repo_stats.comments_made += comment_count;
        }

        for user in &approvals {
            let user_stats = stats
                .entry(user.to_string())
                .or_insert_with(Default::default);

            let total_stats = user_stats
                .entry("All".to_string())
                .or_insert_with(Stats::new);
            total_stats.approvals += 1;

            let repo_stats = user_stats
                .entry(repo.to_string())
                .or_insert_with(Stats::new);
            repo_stats.approvals += 1;
        }
    }

    stats
}

fn get_average_stats(stats: &UserStatistics) -> Stats {
    let mut avg_stats = Stats::new();

    for repos in stats.values() {
        let repo = repos.get("All").expect("Failed to get 'All' row");
        avg_stats.changes += repo.changes;
        avg_stats.approvals += repo.approvals;
        avg_stats.comments_made += repo.comments_made;
        avg_stats.comments_received += repo.comments_received;
        avg_stats.commit_words += repo.commit_words;
        avg_stats.patch_sets += repo.patch_sets;
    }

    let count = stats.len() as u32;
    avg_stats.changes /= count;
    avg_stats.approvals /= count;
    avg_stats.comments_made /= count;
    avg_stats.comments_received /= count;
    avg_stats.commit_words /= count;
    avg_stats.patch_sets /= count;

    avg_stats
}

fn new_csv_writer(filepath: &str) -> csv::Writer<std::fs::File> {
    let mut writer = csv::Writer::from_path(filepath).expect("Failed to create csv writer");

    writer
        .write_record(&[
            "User", "Repo", "CH", "AP", "CM", "CR", "CR/CH", "CW", "CW/CH", "PS", "PS/CH",
        ])
        .expect("Failed to create header record");

    writer
}

fn write_record(writer: &mut csv::Writer<std::fs::File>, user: &str, repo: &str, stats: &Stats) {
    writer
        .write_record(&[
            &user.to_string(),
            &repo.to_string(),
            &stats.changes.to_string(),
            &stats.approvals.to_string(),
            &stats.comments_made.to_string(),
            &stats.comments_received.to_string(),
            &(stats.comments_received as f32 / stats.changes as f32).to_string(),
            &stats.commit_words.to_string(),
            &(stats.commit_words as f32 / stats.changes as f32).to_string(),
            &stats.patch_sets.to_string(),
            &(stats.patch_sets as f32 / stats.changes as f32).to_string(),
        ])
        .expect("Failed to write record to csv file");
}

fn write_simple_stats(stats: &UserStatistics, config: &Config) {
    let mut writer = new_csv_writer("stats.csv");

    let avg_stats = get_average_stats(&stats);
    write_record(&mut writer, "Average", "All", &avg_stats);

    let users = config.user_names();

    for (user, repos) in stats {
        let stats = repos.get("All").expect("Failed to get 'All' row");
        let user_name = &users[user];
        write_record(&mut writer, user_name, "All", &stats);
    }

    writer.flush().expect("Failed to flush writer");
}

fn write_detailed_stats(stats: &UserStatistics, config: &Config) {
    let mut writer = new_csv_writer("detailed.csv");
    let users = config.user_names();

    for (user, repos) in stats {
        let user_name = &users[user];
        for (repo, stats) in repos {
            write_record(&mut writer, user_name, repo, stats);
        }
    }

    writer.flush().expect("Failed to flush writer");
}
