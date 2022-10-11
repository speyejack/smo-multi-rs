use crate::{
    cmds::{Command, ConsoleCommand},
    types::Result,
};
use clap::Parser;
use std::io::Write;
use tokio::sync::{mpsc, oneshot};

// Call this console
#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(subcommand)]
    pub cmd: ConsoleCommand,
}

pub async fn parse_commands(mut to_coord: mpsc::Sender<Command>) -> Result<()> {
    loop {
        let command_result = parse_command(&mut to_coord).await;

        if let Err(e) = command_result {
            println!("{}", e)
        }
    }
}

async fn parse_command(to_coord: &mut mpsc::Sender<Command>) -> Result<()> {
    let task = tokio::task::spawn_blocking(|| async { read_command() });
    let command: Cli = tokio::join!(task).0?.await?;
    let (sender, recv) = oneshot::channel();
    to_coord.send(Command::Console(command.cmd, sender)).await?;
    let result_str = recv.blocking_recv()?;
    let reply_str = result_str?;
    println!("{}", reply_str);

    Ok(())
}

fn read_command() -> Result<Cli> {
    let mut input = "> ".to_string();

    // print!("{}", input);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    tracing::debug!("Got input: {}", input);
    let input = input.trim().split(' ');
    let cli = Cli::try_parse_from(input)?;
    Ok(cli)
}
