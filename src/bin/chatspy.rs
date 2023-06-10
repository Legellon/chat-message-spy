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
enum GetCommand {
    Messages {
        #[arg(short, long)]
        author: Option<String>,
        #[arg(short, long)]
        channel: Option<String>,
    },
}

#[derive(Subcommand, Debug)]
enum CliCommand {
    Start {
        #[arg(short, long, value_parser, num_args=1.., value_delimiter = ',')]
        channels: Option<Vec<String>>,
    },
    Part {
        channels: Option<Vec<String>>,
    },
    Join {
        channels: Vec<String>,
    },
    Add {
        #[command(subcommand)]
        add_command: AddCommand,
    },
    Get {
        #[command(subcommand)]
        get_command: GetCommand,
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
fn parse_part(channels: Option<Vec<String>>) -> Action {
    let a = match channels {
        None => PartAction::All,
        Some(c) => PartAction::Some(c),
    };
    Action::Twitch(TwitchAction::Part(a))
}

#[inline]
fn parse_join(channels: Vec<String>) -> Action {
    Action::Twitch(TwitchAction::Join(channels))
}

#[inline]
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

fn parse_get(a: GetCommand) -> Action {
    match a {
        GetCommand::Messages { author, channel } => {
            Action::Get(GetAction::Messages { author, channel })
        }
    }
}

#[tokio::main]
async fn main() -> IoResult<()> {
    let cli = Args::parse();

    let action = match cli.command {
        CliCommand::Start { channels } => parse_start(channels),
        CliCommand::Part { channels } => parse_part(channels),
        CliCommand::Join { channels } => parse_join(channels),
        CliCommand::Add { add_command } => parse_add(add_command),
        CliCommand::Get { get_command } => parse_get(get_command),
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
        ActionRes::Data(s) => {
            println!("ok; received data:");
            println!("{}", s);
        }
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
