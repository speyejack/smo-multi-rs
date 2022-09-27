use crate::net::Packet;

#[derive(Debug)]
pub enum ClientCommand {
    Packet(Packet),
    SelfAddressed(Packet),
}
