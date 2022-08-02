mod client;
mod cmds;
mod coordinator;
mod guid;
mod net;
mod server;
mod settings;
mod types;

use anyhow::Result;
use clap::Parser;
use client::ClientMap;
use cmds::{Cli, Command};
use coordinator::Coordinator;
use server::Server;
use settings::SyncSettings;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    sync::Arc,
};
use tokio::{
    join,
    sync::{mpsc, RwLock},
};

#[tokio::main]
async fn main() -> Result<()> {
    let (to_coord, from_clients) = mpsc::channel(100);
    let settings = SyncSettings::default();
    let server = Server {
        settings: settings.clone(),
        to_coord: to_coord.clone(),
    };
    let coordinator = Coordinator {
        shine_bag: Arc::new(RwLock::new(HashSet::default())),
        from_clients,
        settings,
        clients: ClientMap::new(),
        to_clients: HashMap::new(),
    };

    let serv_task = tokio::task::spawn(server.listen_for_clients());
    let coord_task = tokio::task::spawn(coordinator.handle_commands());
    let parser_task = tokio::task::spawn(parse_commands(to_coord));

    let _results = tokio::join!(serv_task, coord_task, parser_task);
    Ok(())
}

async fn parse_commands(mut to_coord: mpsc::Sender<Command>) -> Result<()> {
    loop {
        let command_result = parse_command(&mut to_coord).await;

        if let Err(e) = command_result {
            println!("{}", e)
        }
    }
}

async fn parse_command(to_coord: &mut mpsc::Sender<Command>) -> Result<()> {
    let task = tokio::task::spawn_blocking(|| async { read_command() });
    let command: Cli = join!(task).0?.await?;

    Ok(to_coord.send(Command::Cli(command.cmd)).await?)
}

fn read_command() -> Result<Cli> {
    let mut input = "> ".to_string();

    print!("{}", input);
    std::io::stdout().flush()?;
    std::io::stdin().read_line(&mut input)?;
    let input = input.trim().split(' ');
    let cli = Cli::try_parse_from(input)?;
    Ok(cli)
}
