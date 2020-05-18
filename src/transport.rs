use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::thread;
use std::collections::VecDeque;

use crossbeam_channel::{Sender, Receiver, SendError};
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

pub struct NetworkSender {
  sender: Sender<Packet>
}

impl NetworkSender {
  pub fn new(sender: Sender<Packet>) -> Self {
    Self { sender }
  }
}

pub enum NetworkEvent {
  Message(SocketAddr, Bytes),
  Connect(SocketAddr),
  Disconnect(SocketAddr)
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

pub struct EventList(pub Arc<Mutex<Vec<NetworkEvent>>>);

pub fn send_network_system(network: UniqueViewMut<NetworkSender>, mut transport: UniqueViewMut<TransportResource>) {
  for message in &transport.messages {
    for &destination in &message.destination {
      let packet = Packet::reliable_unordered(destination, message.payload.to_vec());
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

pub fn server_receive_network_system(receiver: Receiver<SocketEvent>, client_list: Arc<Mutex<Vec<SocketAddr>>>) -> Receiver<NetworkEvent> {
  let (sender, event_receiver) = crossbeam_channel::unbounded();
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
        sender.send(event).unwrap();
      }
    }
  });
  event_receiver
}

pub fn init_network(world: &mut World, server: &str) -> Result<(), ErrorKind> {
  let mut socket = Socket::bind(server)?;
  let sender = socket.get_packet_sender(); 
  let receiver = socket.get_event_receiver();
  let _thread = thread::spawn(move || socket.start_polling());
  let client_list = ClientList::new();

  // TODO: review event receiver logic, seems we could simplify it a bit
  let events: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(vec![]));
  let events_clone = events.clone();
  let event_list = EventList(events);
  let event_receiver = server_receive_network_system(receiver, client_list.clients.clone());
  thread::spawn(move || {
    loop {
        if let Ok(event) = event_receiver.recv() {
            let mut e = events_clone.lock().unwrap();
            e.push(event);
        }
    }
  });
  let network_sender = NetworkSender::new(sender);
  world.add_unique(network_sender);
  world.add_unique(client_list);
  world.add_unique(event_list);
  world.add_unique(TransportResource::new());
  Ok(())
}

pub fn client_receive_network_system(receiver: Receiver<SocketEvent>, event_list: Arc<Mutex<Vec<NetworkEvent>>>, server: SocketAddr) {
  thread::spawn(move || {
    loop {
      if let Ok(event) = receiver.recv() {
        match event {
          SocketEvent::Packet(packet) if packet.addr() == server => {
            let event = NetworkEvent::Message(
              packet.addr(),
              Bytes::copy_from_slice(packet.payload())
            );
            event_list.lock().unwrap().push(event);
          },
          _ => {}
        };
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
  let events: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(vec![]));
  let event_list = EventList(events);
  
  client_receive_network_system(receiver.clone(), event_list.0.clone(), server);
  let network_sender = NetworkSender::new(sender);
  world.add_unique(network_sender);
  world.add_unique(event_list);
  world.add_unique(TransportResource::new());
  Ok(())
}
