use bytes::{BufMut, BytesMut};



use smoo::net::encoding::{Decodable, Encodable};
use smoo::net::{Packet};

// quickcheck! {
//     fn round_trip(p: Packet) -> bool {
//         let mut buff = BytesMut::with_capacity(1000);
//         p.encode(&mut buff).map(|_| Packet::decode(&mut buff).map(|de_p| de_p == p).unwrap_or(false)).unwrap_or(false)
//     }
// }

#[test]
fn bad_tag_packet() {
    let bad_data = b"~\x80W4\xba-\0\x10\xaf\xed_\xea\xc5h\x15K\x03\0P\00v\xa5E\0\0\xf0B\xa1R\x9fE\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01FlyingWaitR\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\xccL>";

    let mut bytes = BytesMut::with_capacity(150);
    bytes.put(&bad_data[..]);

    println!("Buff len {}", bytes.len());

    let bad_packet = Packet::decode(&mut bytes).unwrap();

    let mut buff = BytesMut::with_capacity(300);
    println!("Buff len {}", buff.len());
    bad_packet.encode(&mut buff).unwrap();
    let decode = Packet::decode(&mut buff).unwrap();
    assert_eq!(bad_packet, decode)
}
