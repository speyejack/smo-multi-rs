use crate::net::Packet;

#[derive(Debug, Clone)]
/// All data commands that can be send to the client
pub enum ClientCommand {
    Packet(Packet),
    SelfAddressed(Packet),
}
