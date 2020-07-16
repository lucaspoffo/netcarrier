use std::collections::HashMap;
use std::collections::VecDeque;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;

use super::{Delta, NetworkController, CarrierDeltaPacket, CarrierPacket};
use bytes::Bytes;
use crossbeam_channel::{Receiver, SendError, Sender};
use laminar::{ErrorKind, Packet, Socket, SocketEvent};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use shipyard::*;

#[derive(Debug, Eq, PartialEq)]
pub struct Message {
    pub destination: Vec<SocketAddr>,
    pub payload: Bytes,
    pub delivery: DeliveryRequirement,
}

impl Message {
    pub fn new(
        destination: Vec<SocketAddr>,
        payload: &[u8],
        delivery: DeliveryRequirement,
    ) -> Self {
        Self {
            destination,
            payload: Bytes::copy_from_slice(payload),
            delivery,
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
    state: Vec<u8>,
}

#[derive(Debug, Eq, PartialEq)]
pub enum DeliveryRequirement {
    Unreliable,
    UnreliableSequenced(Option<u8>),
    Reliable,
    ReliableSequenced(Option<u8>),
    ReliableOrdered(Option<u8>),
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
                state: message.payload.to_vec(),
            };
            let payload = bincode::serialize(&net_state).unwrap();
            let packet = match message.delivery {
                DeliveryRequirement::Reliable => Packet::reliable_unordered(destination, payload),
                DeliveryRequirement::Unreliable => Packet::unreliable(destination, payload),
                DeliveryRequirement::UnreliableSequenced(stream_id) => {
                    Packet::unreliable_sequenced(destination, payload, stream_id)
                }
                DeliveryRequirement::ReliableSequenced(stream_id) => {
                    Packet::reliable_sequenced(destination, payload, stream_id)
                }
                DeliveryRequirement::ReliableOrdered(stream_id) => {
                    Packet::reliable_ordered(destination, payload, stream_id)
                }
            };

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
            println!("{}", message.payload.len());
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
                    if let Ok(net_client_state) =
                        bincode::deserialize::<NetworkClientState>(&packet.payload())
                    {
                        // println!("Client {}, last_received frame: {}", packet.addr(), net_client_state.ack.last_frame);
                        NetworkEvent::Message(
                            packet.addr(),
                            Bytes::copy_from_slice(&net_client_state.state),
                        )
                    } else {
                        break;
                    }
                }
                SocketEvent::Connect(addr) => {
                    let mut clients = client_list.lock().unwrap();
                    println!("Client {} connected!", addr);
                    if !clients.contains(&addr) {
                        clients.push(addr);
                    }
                    NetworkEvent::Connect(addr)
                }
                SocketEvent::Timeout(addr) => {
                    println!("Client {} disconnected!", addr);
                    client_list.lock().unwrap().retain(|&x| x != addr);
                    NetworkEvent::Disconnect(addr)
                }
            };
            sender.send(event).unwrap();
        }
    });
    event_receiver
}

pub fn init_network<T>(world: &mut World, server: &str) -> Result<(), ErrorKind>
where
    T: 'static + DeserializeOwned + CarrierPacket + Sync + Send + Delta + Serialize + Clone + Debug,
    T::DeltaType: CarrierDeltaPacket + Debug,
{
    let mut socket = Socket::bind(server)?;
    let sender = socket.get_packet_sender();
    let receiver = socket.get_event_receiver();
    let _thread = thread::spawn(move || socket.start_polling());
    let client_list = ClientList::default();
    let network_controller = NetworkController::new(10);

    // TODO: review event receiver logic, seems we could simplify it a bit
    let events: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(vec![]));
    let snapshot = GameSnapshot(Arc::new(Mutex::new(T::new(&world, 0))));

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
    world.add_unique(snapshot);
    world.add_unique(network_controller);
    world.add_unique(client_list);
    world.add_unique(event_list);
    world.add_unique(TransportResource::default());
    Ok(())
}

#[derive(Serialize, Deserialize, Debug)]
pub enum ServerMessage<T>
where
    T: Delta,
{
    Snapshot(T),
    Delta(T::DeltaType),
}

pub fn client_receive_network_system<T>(
    receiver: Receiver<SocketEvent>,
    jit_buffer: Arc<Mutex<Vec<T>>>,
    snapshots: Arc<Mutex<Vec<T>>>,
    network_client_ack: Arc<Mutex<NetworkClientAck>>,
    server: SocketAddr,
) where
    T: 'static + DeserializeOwned + CarrierPacket + Sync + Send + Delta + Clone + Debug,
    T::DeltaType: CarrierDeltaPacket + Debug,
{
    thread::spawn(move || loop {
        if let Ok(event) = receiver.recv() {
            match event {
                // TODO: match every socket event type
                SocketEvent::Packet(packet) if packet.addr() == server => {
                    if let Ok(net_state) =
                        bincode::deserialize::<ServerMessage<T>>(&packet.payload())
                    {
                        let mut ack = network_client_ack.lock().unwrap();
                        let mut jit_buffer = jit_buffer.lock().unwrap();
                        match net_state {
                            ServerMessage::Snapshot(snapshot) => {
                                ack.last_snapshot_frame = snapshot.frame();
                                ack.last_frame = snapshot.frame();

                                let mut snapshots = snapshots.lock().unwrap();
                                jit_buffer.push(snapshot.clone());
                                if snapshots.len() == 2 {
                                    //TODO: meh it works for now, fixed 2 length
                                    if snapshots[0].frame() < snapshots[1].frame() {
                                        snapshots[0] = snapshot
                                    } else {
                                        snapshots[1] = snapshot
                                    }
                                } else {
                                    snapshots.push(snapshot);
                                }
                            }
                            ServerMessage::Delta(delta) => {
                                let snapshots = snapshots.lock().unwrap();
                                ack.last_frame = delta.frame();
                                //TODO: if we don't find the snapshot we should the save the delta to apply when we get the snapshot
                                if let Some(snapshot) = snapshots
                                    .iter()
                                    .find(|s| s.frame() == delta.snapshot_frame())
                                {
                                    jit_buffer.push(snapshot.apply(&delta));
                                }
                            }
                        }
                        jit_buffer.sort_by(|a, b| a.frame().cmp(&b.frame()));
                    }
                }
                _ => {}
            };
        }
    });
}

//TODO: pass types to Packet
pub fn init_client_network<T>(world: &mut World, addr: &str, server: &str) -> Result<(), ErrorKind>
where
    T: 'static + DeserializeOwned + CarrierPacket + Sync + Send + Delta + Serialize + Clone + Debug,
    T::DeltaType: CarrierDeltaPacket + Debug,
{
    let mut socket = Socket::bind(addr)?;
    let server = server.parse().unwrap();
    let net_id_mapping = NetworkIdMapping(HashMap::new());
    let sender = socket.get_packet_sender();
    let receiver = socket.get_event_receiver();
    let _thread = thread::spawn(move || socket.start_polling());
    let buffer: Arc<Mutex<Vec<T>>> = Arc::new(Mutex::new(vec![]));
    let network_client_ack = Arc::new(Mutex::new(NetworkClientAck {
        last_frame: 0,
        last_snapshot_frame: 0,
    }));
    let network_ack = NetworkAck(network_client_ack);
    let jit_buffer = JitBuffer(buffer);
    let snapshots = ClientGameSnapshots(Arc::new(Mutex::new(vec![T::new(&world, 0)])));

    client_receive_network_system::<T>(
        receiver,
        jit_buffer.0.clone(),
        snapshots.0.clone(),
        network_ack.0.clone(),
        server,
    );
    let network_sender = NetworkSender::new(sender);
    world.add_unique(net_id_mapping);
    world.add_unique(snapshots);
    world.add_unique(network_ack);
    world.add_unique(network_sender);
    world.add_unique(jit_buffer);
    world.add_unique(TransportResource::default());
    Ok(())
}

pub struct GameSnapshot<T>(pub Arc<Mutex<T>>) 
where T: 'static + Sync + Send + CarrierPacket + Serialize, T::DeltaType: CarrierDeltaPacket;
pub struct ClientGameSnapshots<T>(
    pub Arc<Mutex<Vec<T>>>,
) where T: 'static + Sync + Send + CarrierPacket + Serialize, T::DeltaType: CarrierDeltaPacket;

pub fn update_server<T>(world: &mut World, frame: u32,) 
where T: 'static + Sync + Send + CarrierPacket + Serialize + Clone, T::DeltaType: CarrierDeltaPacket {
    let net_state = T::new(&world, frame);
    world.run(
        |client_list: UniqueView<ClientList>,
         mut transport: UniqueViewMut<TransportResource>,
         mut snapshot: UniqueViewMut<GameSnapshot<T>>,
         mut network_controller: UniqueViewMut<NetworkController>| {
            network_controller.tick();
            if network_controller.is_snapshot_frame() {
                *snapshot.0.lock().unwrap() = net_state.clone();
                let server_message = ServerMessage::<T>::Snapshot(net_state);
                let payload = bincode::serialize(&server_message).unwrap();
                println!("Netpacket len: {:?}", payload.len());
                transport.messages.push_back(Message::new(
                    client_list.clients.lock().unwrap().clone(),
                    &payload[..],
                    DeliveryRequirement::Unreliable,
                ));
            } else {
                let snapshot = snapshot.0.lock().unwrap();
                let delta_packet = net_state.from(&snapshot);
                let server_message = ServerMessage::<T>::Delta(delta_packet);
                let payload = bincode::serialize(&server_message).unwrap();
                println!("Netpacket len: {:?}", payload.len());
                transport.messages.push_back(Message::new(
                    client_list.clients.lock().unwrap().clone(),
                    &payload[..],
                    DeliveryRequirement::ReliableSequenced(Some(1)),
                ));
            }
        },
    );
    world.run(server_send_network_system);
}

pub fn update_client<T: Serialize>(world: &mut World, client_state: T, server: SocketAddr) {
    let encoded_client: Vec<u8> = bincode::serialize(&client_state).unwrap();
    world.run(|mut transport: UniqueViewMut<TransportResource>| {
        transport.messages.push_back(Message::new(
            vec![server],
            &encoded_client,
            DeliveryRequirement::Unreliable,
        ));
    });
    world.run(client_send_network_system);
}
