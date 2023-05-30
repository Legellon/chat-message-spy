use chatspy::match_pattern::MatchMode;
use chatspy::protocol::*;
use chatspy::SOCKET_PATH;
use clap::{Parser, Subcommand};
use tokio::io::Result as IoResult;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: CliCommand,
}

#[derive(Subcommand, Debug)]
enum AddCommand {
    Pattern {
        name: String,
        #[arg(short, long, value_parser, num_args=1.., value_delimiter = ',')]
        words: Vec<String>,
        #[arg(short, long)]
        default: Option<bool>,
        #[arg(short, long, value_enum)]
        mode: Option<MatchMode>,
    },
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    Start {
        #[arg(short, long, value_parser, num_args=1.., value_delimiter = ',')]
        channels: Option<Vec<String>>,
    },
    Stop {
        channel: Option<String>,
        #[arg(short, long, value_parser, num_args=1.., value_delimiter = ',')]
        channels: Option<Vec<String>>,
    },
    Join {
        channels: Vec<String>,
    },
    Add {
        #[command(subcommand)]
        add_command: AddCommand,
    },
}

#[inline]
fn parse_start(chs: Option<Vec<String>>) -> Action {
    if let Some(chs) = chs {
        Action::Twitch(TwitchAction::Start(StartAction::Prejoin(chs)))
    } else {
        Action::Twitch(TwitchAction::Start(StartAction::Simple))
    }
}

#[inline]
fn parse_stop(ch: Option<String>, chs: Option<Vec<String>>) -> Action {
    match (ch, chs) {
        (None, None) => Action::Twitch(TwitchAction::Part(PartAction::All)),
        (Some(ch), _) => Action::Twitch(TwitchAction::Part(PartAction::One(ch))),
        (_, Some(chs)) => Action::Twitch(TwitchAction::Part(PartAction::Many(chs))),
    }
}

#[inline]
fn parse_join(chs: Vec<String>) -> Action {
    if chs.len() == 1 {
        Action::Twitch(TwitchAction::Join(JoinAction::One(chs[0].clone())))
    } else {
        Action::Twitch(TwitchAction::Join(JoinAction::Many(chs)))
    }
}

fn parse_add(a: AddCommand) -> Action {
    match a {
        AddCommand::Pattern {
            name,
            words,
            default,
            mode,
        } => Action::Add(AddAction::Pattern {
            raw_pattern: (words, mode.unwrap_or_default()),
            name,
            default: default.unwrap_or_default(),
        }),
    }
}

#[tokio::main]
async fn main() -> IoResult<()> {
    let cli = Args::parse();

    let action = match cli.command {
        CliCommand::Start { channels } => parse_start(channels),
        CliCommand::Stop { channel, channels } => parse_stop(channel, channels),
        CliCommand::Join { channels } => parse_join(channels),
        CliCommand::Add { add_command } => parse_add(add_command),
    };

    let res = execute_action(action).await?;

    match res {
        ActionRes::Failure { errors, level } => match level {
            FailureLevel::Critical => {
                eprintln!("failed: critical errors occurred");
                for e in errors {
                    eprintln!("{}", e);
                }
            }
            FailureLevel::Uncritical => {
                eprintln!("warning: uncritical errors occurred");
                for e in errors {
                    eprintln!("{}", e);
                }
            }
        },
        ActionRes::Success => println!("ok"),
    }

    Ok(())
}

#[inline]
async fn execute_action(action: Action) -> IoResult<ActionRes> {
    let mut us = UnixStream::connect(SOCKET_PATH).await?;

    let buf = serde_json::to_vec(&action)?;
    us.write_all(&buf).await?;
    us.shutdown().await?;

    let mut buf = vec![];
    us.read_to_end(&mut buf).await?;
    let action_res = serde_json::from_slice::<ActionRes>(&buf)?;

    Ok(action_res)
}
