use crate::net::Packet;

#[derive(Debug, Clone)]
pub enum ClientCommand {
    Packet(Packet),
    SelfAddressed(Packet),
}
