pub mod client;
pub mod console;
pub mod coord;
pub mod reply;

pub use client::ClientCommand;
pub use console::ConsoleCommand;
pub use coord::ServerCommand;

use crate::net::Packet;

#[derive(Debug)]
pub enum Command {
    Packet(Packet),
    Console(ConsoleCommand),
    Server(ServerCommand),
}
