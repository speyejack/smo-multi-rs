use bytes::{Buf, BufMut, BytesMut};


use smoo::guid::Guid;
use smoo::net::encoding::{Decodable, Encodable};
use smoo::net::{Packet, PacketData, TagUpdate};

// Used to test any bad packet decodes
#[ignore]
#[test_log::test]
fn bad_data_packet() {
    let bad_data = b"~\x80W4\xba-\0\x10\xaf\xed_\xea\xc5h\x15K\x03\0P\00v\xa5E\0\0\xf0B\xa1R\x9fE\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\x01FlyingWaitR\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\0\xccL>";

    let mut bytes = BytesMut::with_capacity(150);
    bytes.put(&bad_data[..]);

    println!("Buff len {}", bytes.len());

    let mut bad_packet = Packet::decode(&mut bytes).unwrap();

    bad_packet.resize();

    let mut buff = BytesMut::with_capacity(300);
    println!("Buff len {}", buff.len());
    bad_packet.encode(&mut buff).unwrap();

    let buff = buff.freeze();
    // println!("C: {}", buff.get_u8());
    println!("Buff decode: {:?} ({})", buff, buff.len());
    let decode = Packet::decode(&mut (&buff[..])).unwrap();

    assert_eq!(bad_packet, decode)
}

// Used to test any bad packet round trip issues
#[ignore]
#[test_log::test]
fn bad_packet() {
    let mut bad_packet = Packet {
        id: Guid {
            id: [
                211, 55, 133, 91, 69, 255, 239, 214, 220, 209, 51, 243, 52, 26, 154, 27,
            ],
        },
        data_size: 6,
        data: PacketData::Tag {
            update_type: TagUpdate::State,
            is_it: true,
            seconds: 115,
            minutes: 32608,
        },
    };
    bad_packet.resize();

    let mut buff = BytesMut::with_capacity(1000);

    bad_packet.encode(&mut buff).expect("Encode error");
    tracing::debug!("Encode buff: {:?} ({})", buff, buff.remaining());
    let new_pack = Packet::decode(&mut buff).expect("Decode error");
    assert_eq!(bad_packet, new_pack);
}
