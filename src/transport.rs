use std::net::SocketAddr;
use std::time::Instant;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::VecDeque;

use crossbeam_channel::{Sender, Receiver, SendError};
use crossbeam_queue::SegQueue;
use laminar::{Socket, SocketEvent, Packet, ErrorKind};
use bytes::Bytes;
use shipyard::*; 

#[derive(Debug, Eq, PartialEq)]
pub struct Message {
  pub destination: Vec<SocketAddr>,
  pub payload: Bytes
}

impl Message {
  pub fn new(destination: Vec<SocketAddr>, payload: &[u8]) -> Self {
    Self {
      destination,
      payload: Bytes::copy_from_slice(payload)
    }
  }
}

pub struct NetcarrierNetwork {
  // socket: Socket
  sender: Sender<Packet>,
  receiver: Receiver<SocketEvent>
}

impl NetcarrierNetwork {
  pub fn new(sender: Sender<Packet>, receiver: Receiver<SocketEvent>) -> Self {
    Self { sender, receiver }
  }
}

pub enum NetworkEvent {
  Message(SocketAddr, Bytes),
  Connect(SocketAddr),
  Disconnect(SocketAddr)
}

pub struct EventQueue {
  pub events: Arc<SegQueue<NetworkEvent>>
}

impl EventQueue {
  pub fn new() -> Self {
    Self { events: Arc::new(SegQueue::new()) }
  }  
}

pub struct TransportResource {
  pub messages: VecDeque<Message>
}

impl TransportResource {
  pub fn new() -> Self {
    Self { messages: VecDeque::new() }
  }
}

pub struct ClientList {
  pub clients:  Arc<Mutex<Vec<SocketAddr>>>
}

impl ClientList {
  pub fn new() -> Self {
    Self { clients: Arc::new(Mutex::new(Vec::new())) }
  }
}

pub fn send_network_system(network: UniqueViewMut<NetcarrierNetwork>, mut transport: UniqueViewMut<TransportResource>) {
  for message in &transport.messages {
    println!("Destination: {:?}", message.destination);
    for &destination in &message.destination {
      let packet = Packet::reliable_unordered(destination, message.payload.to_vec());
      // match network.socket.send(packet) {
      match network.sender.send(packet) {
        Err(SendError(e)) => {
          println!("Send Error sending message: {:?}", e);
        },
        Ok(_) => {}
      }
    }
  }
  transport.messages.clear();
}

pub fn receive_network_system(receiver: Receiver<SocketEvent>, event_queue: Arc<SegQueue<NetworkEvent>>, client_list: Arc<Mutex<Vec<SocketAddr>>>) {
  let _pool = thread::spawn(move || {
    loop {
      if let Ok(event) = receiver.recv() {
        let event = match event {
          SocketEvent::Packet(packet) => NetworkEvent::Message(
            packet.addr(),
            Bytes::copy_from_slice(packet.payload())
          ),
          SocketEvent::Connect(addr) => {
            let mut clients = client_list.lock().unwrap();
            if !clients.contains(&addr) {
              clients.push(addr);
            }
            NetworkEvent::Connect(addr)
          },
          SocketEvent::Timeout(addr) => {
            client_list.lock().unwrap().retain(|&x| x != addr);
            NetworkEvent::Disconnect(addr)
          }
        };
        event_queue.push(event);
        println!("Transport EventQueue: {:?}",  event_queue.len());
      }
    }
  });
}

pub fn init_network(world: &mut World, server: &str) -> Result<(), ErrorKind> {
  let mut socket = Socket::bind(server)?;
  let sender = socket.get_packet_sender(); 
  let receiver = socket.get_event_receiver();
  let _thread = thread::spawn(move || socket.start_polling());
  let event_queue = EventQueue::new();
  let client_list = ClientList::new();
  receive_network_system(receiver.clone(), event_queue.events.clone(), client_list.clients.clone());
  let network = NetcarrierNetwork::new(sender, receiver);
  world.add_unique(network);
  world.add_unique(client_list);
  world.add_unique(event_queue);
  world.add_unique(TransportResource::new());
  Ok(())
}

pub fn client_receive_network_system(receiver: Receiver<SocketEvent>, event_queue: Arc<SegQueue<NetworkEvent>>, server: SocketAddr) {
  let _pool = thread::spawn(move || {
    loop {
      if let Ok(event) = receiver.recv() {
        match event {
          SocketEvent::Packet(packet) if packet.addr() == server => {
            let event = NetworkEvent::Message(
              packet.addr(),
              Bytes::copy_from_slice(packet.payload())
            );
            event_queue.push(event);
          },
          _ => {}
        };
        println!("Transport EventQueue: {:?}",  event_queue.len());
      }
    }
  });
}

pub fn init_client_network(world: &mut World, addr: &str, server: &str) -> Result<(), ErrorKind> {
  let mut socket = Socket::bind(addr)?;
  let server = server.parse().unwrap();

  let sender = socket.get_packet_sender(); 
  let receiver = socket.get_event_receiver();
  let _thread = thread::spawn(move || socket.start_polling());
  
  let event_queue = EventQueue::new();
  client_receive_network_system(receiver.clone(), event_queue.events.clone(), server);
  let network = NetcarrierNetwork::new(sender, receiver);
  world.add_unique(network);
  world.add_unique(event_queue);
  world.add_unique(TransportResource::new());
  Ok(())
}
