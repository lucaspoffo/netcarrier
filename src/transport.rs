use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::HashMap;

use bytes::Bytes;
use crossbeam_channel::{Receiver, SendError, Sender};
use laminar::{ErrorKind, Packet, Socket, SocketEvent};
use shipyard::*;
use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use super::NetworkFrame;

#[derive(Debug, Eq, PartialEq)]
pub struct Message {
    pub destination: Vec<SocketAddr>,
    pub payload: Bytes,
}

impl Message {
    pub fn new(destination: Vec<SocketAddr>, payload: &[u8]) -> Self {
        Self {
            destination,
            payload: Bytes::copy_from_slice(payload),
        }
    }
}

pub struct NetworkSender {
    sender: Sender<Packet>,
}

impl NetworkSender {
    pub fn new(sender: Sender<Packet>) -> Self {
        Self { sender }
    }
}

pub enum NetworkEvent {
    Message(SocketAddr, Bytes),
    Connect(SocketAddr),
    Disconnect(SocketAddr),
}

#[derive(Default)]
pub struct TransportResource {
    pub messages: VecDeque<Message>,
}

#[derive(Default)]
pub struct ClientList {
    pub clients: Arc<Mutex<Vec<SocketAddr>>>,
}

pub struct EventList(pub Arc<Mutex<Vec<NetworkEvent>>>);

pub struct JitBuffer<T>(pub Arc<Mutex<Vec<T>>>);

pub struct NetworkIdMapping(pub HashMap<u32, EntityId>);

#[derive(Serialize, Deserialize)]
pub struct NetworkClientState {
	ack: NetworkClientAck,
	state: Vec<u8>
}

// TODO: review struct name and struct alias
#[derive(Clone, Serialize, Deserialize)]
pub struct NetworkClientAck {
    last_frame: u32,
	last_snapshot_frame: u32,
}

pub struct NetworkAck(pub Arc<Mutex<NetworkClientAck>>);

pub fn client_send_network_system(
    network: UniqueViewMut<NetworkSender>,
    mut transport: UniqueViewMut<TransportResource>,
    network_ack: UniqueViewMut<NetworkAck>,
) {
    let ack = network_ack.0.lock().unwrap();
    for message in &transport.messages {
        for &destination in &message.destination {
            let net_state = NetworkClientState {
                ack: ack.clone(),
                state: message.payload.to_vec()
            };
            let packet = Packet::reliable_unordered(destination, bincode::serialize(&net_state).unwrap());
            if let Err(SendError(e)) = network.sender.send(packet) {
                println!("Send Error sending message: {:?}", e);
            }
        }
    }
    transport.messages.clear();
}

pub fn server_send_network_system(
    network: UniqueViewMut<NetworkSender>,
    mut transport: UniqueViewMut<TransportResource>,
) {
    for message in &transport.messages {
        for &destination in &message.destination {
            let packet = Packet::reliable_unordered(destination, message.payload.to_vec());
            if let Err(SendError(e)) = network.sender.send(packet) {
                println!("Send Error sending message: {:?}", e);
            }
        }
    }
    transport.messages.clear();
}

pub fn server_receive_network_system(
    receiver: Receiver<SocketEvent>,
    client_list: Arc<Mutex<Vec<SocketAddr>>>,
) -> Receiver<NetworkEvent> {
    let (sender, event_receiver) = crossbeam_channel::unbounded();
    let _pool = thread::spawn(move || loop {
        if let Ok(event) = receiver.recv() {
            let event = match event {
                SocketEvent::Packet(packet) => {
                    if let Ok(net_client_state) = bincode::deserialize::<NetworkClientState>(&packet.payload()) {
                        println!("Client {}, last_received frame: {}", packet.addr(), net_client_state.ack.last_frame);
                        NetworkEvent::Message(packet.addr(), Bytes::copy_from_slice(&net_client_state.state))
                    } else {
                        break;
                    }
                }
                SocketEvent::Connect(addr) => {
                    let mut clients = client_list.lock().unwrap();
                    if !clients.contains(&addr) {
                        clients.push(addr);
                    }
                    NetworkEvent::Connect(addr)
                }
                SocketEvent::Timeout(addr) => {
                    client_list.lock().unwrap().retain(|&x| x != addr);
                    NetworkEvent::Disconnect(addr)
                }
            };
            sender.send(event).unwrap();
        }
    });
    event_receiver
}

pub fn init_network(world: &mut World, server: &str) -> Result<(), ErrorKind> {
    let mut socket = Socket::bind(server)?;
    let sender = socket.get_packet_sender();
    let receiver = socket.get_event_receiver();
    let _thread = thread::spawn(move || socket.start_polling());
    let client_list = ClientList::default();

    // TODO: review event receiver logic, seems we could simplify it a bit
    let events: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = events.clone();
    let event_list = EventList(events);
    let event_receiver = server_receive_network_system(receiver, client_list.clients.clone());
    thread::spawn(move || loop {
        if let Ok(event) = event_receiver.recv() {
            let mut e = events_clone.lock().unwrap();
            e.push(event);
        }
    });
    let network_sender = NetworkSender::new(sender);
    world.add_unique(network_sender);
    world.add_unique(client_list);
    world.add_unique(event_list);
    world.add_unique(TransportResource::default());
    Ok(())
}

pub fn client_receive_network_system<T: 'static + DeserializeOwned + NetworkFrame + Sync + Send>(
    receiver: Receiver<SocketEvent>,
    jit_buffer: Arc<Mutex<Vec<T>>>,
    network_client_ack: Arc<Mutex<NetworkClientAck>>,
    server: SocketAddr,
) {
    thread::spawn(move || loop {
        if let Ok(event) = receiver.recv() {
            match event {
                // TODO: match every socket event type
                SocketEvent::Packet(packet) if packet.addr() == server => {
                    if let Ok(net_state) = bincode::deserialize::<T>(&packet.payload()) {
                        let mut jit_buffer = jit_buffer.lock().unwrap();
                        jit_buffer.push(net_state);
                        jit_buffer.sort_by(|a, b| a.frame().cmp(&b.frame()));
                        let mut ack = network_client_ack.lock().unwrap();
                        ack.last_snapshot_frame = jit_buffer[jit_buffer.len() - 1].frame();
                        ack.last_frame = jit_buffer[jit_buffer.len() - 1].frame();
                    }                    
                }
                _ => {}
            };
        }
    });
}

pub fn init_client_network<T: 'static + DeserializeOwned + NetworkFrame + Sync + Send>(world: &mut World, addr: &str, server: &str) -> Result<(), ErrorKind> {
    let mut socket = Socket::bind(addr)?;
    let server = server.parse().unwrap();
    let net_id_mapping = NetworkIdMapping(HashMap::new());
    let sender = socket.get_packet_sender();
    let receiver = socket.get_event_receiver();
    let _thread = thread::spawn(move || socket.start_polling());
    let buffer: Arc<Mutex<Vec<T>>> = Arc::new(Mutex::new(vec![]));
    let network_client_ack = Arc::new(Mutex::new(NetworkClientAck { last_frame: 0, last_snapshot_frame: 0 }));
    let network_ack = NetworkAck(network_client_ack);
    let jit_buffer = JitBuffer(buffer);
    
    client_receive_network_system::<T>(receiver, jit_buffer.0.clone(), network_ack.0.clone(), server);
    let network_sender = NetworkSender::new(sender);
    world.add_unique(net_id_mapping);
    world.add_unique(network_ack);
    world.add_unique(network_sender);
    world.add_unique(jit_buffer);
    world.add_unique(TransportResource::default());
    Ok(())
}
