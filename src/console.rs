use crate::{
    cmds::{Command, ConsoleCommand, ServerWideCommand},
    types::Result,
};
use clap::Parser;
use std::io::Write;
use tokio::{
    select,
    sync::{broadcast, mpsc, oneshot},
};

// Call this console
#[derive(Parser, Debug)]
pub struct Cli {
    #[clap(subcommand)]
    pub cmd: ConsoleCommand,
}

pub async fn parse_commands(
    mut to_coord: mpsc::Sender<Command>,
    mut server_cmds: broadcast::Receiver<ServerWideCommand>,
) -> Result<()> {
    loop {
        // let command_result = parse_command(&mut to_coord).await;
        let command_result = select! {
            result = parse_command(&mut to_coord) => {
                result
            },
            exit_cmd = server_cmds.recv() => {
                match exit_cmd? {
                    ServerWideCommand::Shutdown => break Ok(())
                }
            }

        };

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
    let result_str = recv.await?;
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
