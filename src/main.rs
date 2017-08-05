extern crate etclient_core as core;
extern crate etcommon_crypto;
extern crate etcommon_bigint as bigint;
extern crate etcommon_rlp as rlp;
extern crate rlpx;
extern crate dpt;
extern crate rand;
extern crate secp256k1;
extern crate futures;
extern crate tokio_io;
extern crate tokio_core;

use futures::{IntoFuture, Future, Stream, Sink, Async, future};
use tokio_core::reactor::Core;
use std::io;
use rlpx::{RLPxStream, CapabilityInfo};
use dpt::{DPTNode, DPTStream, DPTMessage};
use secp256k1::key::{PublicKey, SecretKey};
use etcommon_crypto::SECP256K1;
use bigint::H512;
use rand::os::OsRng;
use std::str::FromStr;

const BOOTSTRAP_ID: &str = "42d8f29d1db5f4b2947cd5c3d76c6d0d3697e6b9b3430c3d41e46b4bb77655433aeedc25d4b4ea9d8214b6a43008ba67199374a9b53633301bca0cd20c6928ab";
const BOOTSTRAP_IP: &str = "104.155.176.151";
const BOOTSTRAP_PORT: u16 = 30303;

fn main() {
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let addr = "0.0.0.0:30303".parse().unwrap();
    let secret_key = SecretKey::new(&SECP256K1, &mut OsRng::new().unwrap());

    let mut dpt = DPTStream::new(
        &addr, &handle, secret_key.clone(),
        vec![DPTNode {
            address: BOOTSTRAP_IP.parse().unwrap(),
            tcp_port: BOOTSTRAP_PORT,
            udp_port: BOOTSTRAP_PORT,
            id: H512::from_str(BOOTSTRAP_ID).unwrap(),
        }], 0).unwrap();

    let mut rlpx = RLPxStream::new(
        &handle, secret_key.clone(),
        4, "etclient Rust/0.1.0".to_string(),
        vec![CapabilityInfo { name: "eth", version: 62, length: 8 },
             CapabilityInfo { name: "eth", version: 63, length: 17 }],
        0);

    let mut wait = vec![
        Box::new(dpt.send(DPTMessage::RequestNewPeer).and_then(|client| {
            client.into_future().map_err(|(e, _)| e)
        })),
        Box::new(rlpx.into_future()),
    ];

    loop {
        let ((val, n), v) = core.run(future::select_ok(wait)).unwrap();

        println!("returned, {:?}", val);

        wait = v;
    }

    println!("Hello, world!");
}
