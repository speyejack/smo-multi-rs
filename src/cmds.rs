pub mod client;
pub mod console;
pub mod coord;
pub mod reply;

pub use client::ClientCommand;
pub use console::ConsoleCommand;
pub use coord::ServerCommand;

use crate::{net::Packet, types::SMOError};

use self::reply::ReplyChannel;

#[derive(Debug)]
pub enum Command {
    Packet(Packet),
    Console(ConsoleCommand, ReplyChannel<Result<String, SMOError>>),
    Server(ServerCommand),
}

#[derive(Debug, Clone)]
pub enum ServerWideCommand {
    Shutdown,
}
