use std::{thread, time::{self, Duration}};
use std::net::SocketAddr;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use laminar::{ErrorKind};
use shipyard::*;

use netcarrier::{Game, Position, Velocity, NetworkIdentifier, ClientState, Color};
use netcarrier::transport::{self, ClientList, TransportResource, Message, EventQueue, NetworkEvent};

const MS_PER_FRAME: u64 = 50;
const SERVER: &str = "127.0.0.1:12351";

struct GameState(Vec<u8>);
struct EventList(Arc<Mutex<Vec<NetworkEvent>>>);

#[allow(unreachable_code)]
pub fn init() -> Result<(), ErrorKind> {    
    let mut game = Game::new_empty();
    let event_receiver = transport::init_network(&mut game.world, SERVER)?;
    let events: Arc<Mutex<Vec<NetworkEvent>>> = Arc::new(Mutex::new(vec![]));
    let events_clone = events.clone();
    let event_list = EventList(events);
    thread::spawn(move || {
        loop {
            if let Ok(event) = event_receiver.recv() {
                let mut e = events_clone.lock().unwrap();
                e.push(event);
            }
        }
    });

    game.world.add_unique(GameState(vec![]));
    game.world.add_unique(ClientMapper::new());
    game.world.add_unique(event_list);
    
    loop {
        game.tick();
        println!("frame: {}", game.frame);

        let start = time::Instant::now();
        game.world.run(process_events);
        game.world.run(system_update_player);
        game.world.run(system_move);
        
        let game_encoded: Vec<u8> = game.encoded();
        game.world.run(|mut game_state: UniqueViewMut<GameState>| {
            println!("GameState: {:?}", game_encoded);
            game_state.0 = game_encoded;
        });

        game.world.run(add_players_packets);
        game.world.run(transport::send_network_system);

        let now = time::Instant::now();
        let frame_duration = time::Duration::from_millis(MS_PER_FRAME);
        if let Some(wait) = (start + frame_duration).checked_duration_since(now) {
            thread::sleep(wait);
        }
    }

    Ok(())
}

fn system_move(mut posisitons: ViewMut<Position>, velocities: View<Velocity>) {
    for (pos, vel) in (&mut posisitons, &velocities).iter() {
        pos.x += vel.dx * 10.0;
        pos.y += vel.dy * 10.0;
        println!("{:?}", pos);
    }
}

fn add_players_packets(client_list: UniqueView<ClientList>, mut transport: UniqueViewMut<TransportResource>, game_state: UniqueView<GameState>) {
    transport.messages.push_back(Message::new(client_list.clients.lock().unwrap().clone(), &game_state.0[..]));
}

fn system_update_player(clients_state: View<ClientState>, mut velocities: ViewMut<Velocity>) {
    for (state, velocity) in (&clients_state, &mut velocities).iter() {
        velocity.dx = (state.right as i32 - state.left as i32) as f32;
        velocity.dy = (state.down as i32 - state.up as i32) as f32;
    }
}

fn process_events(mut all_storages: AllStoragesViewMut) {
    let mut removed_entities: Vec<EntityId> = vec![];
    {
        let mut entities = all_storages.borrow::<EntitiesViewMut>();
        let mut event_list = all_storages.borrow::<UniqueViewMut<EventList>>();
        let mut client_mapper = all_storages.borrow::<UniqueViewMut<ClientMapper>>();
        let mut positions = all_storages.borrow::<ViewMut<Position>>();
        let mut colors = all_storages.borrow::<ViewMut<Color>>();
        let mut velocities = all_storages.borrow::<ViewMut<Velocity>>();
        let mut clients_state = all_storages.borrow::<ViewMut<ClientState>>();
        let mut net_ids = all_storages.borrow::<ViewMut<NetworkIdentifier>>();
        let mut event_list = event_list.0.lock().unwrap();
        println!("EventList: {:?}",  event_list.len());
        // for event in &event_queue.events {
        // while let Ok(event) = event_queue.events.pop() {
        event_list.drain(..).for_each(|event| {
            match event {
                NetworkEvent::Connect(addr) => {
                    println!("Client {} connected.", addr);
                    if let Some(_) = client_mapper.get(&addr) {
                        return;
                    }

                    let net_id = NetworkIdentifier::new();
                    let entity = entities.add_entity(
                        (&mut positions, &mut velocities, &mut net_ids, &mut clients_state, &mut colors),
                        (Position::new(100.0, 100.0), Velocity::new(0.0, 0.0), net_id, ClientState::default(), Color::random())
                    );
                    client_mapper.insert(addr.clone(), entity);
                },
                NetworkEvent::Disconnect(addr) => {
                    println!("Client {} disconnected.", addr);
                    if let Some(entity_id) = client_mapper.remove(&addr) {
                        removed_entities.push(entity_id);
                    }
                },
                NetworkEvent::Message(addr, bytes) => {
                    if let Ok(decoded) = bincode::deserialize::<ClientState>(&bytes) {
                        println!("Received {:?} from {}.", decoded, addr);
                        if let Some(entity_id) = client_mapper.get(&addr) {
                            clients_state[*entity_id] = decoded;
                        }
                    }
                },
            }
        });
    }
    for entity_id in removed_entities {
        all_storages.delete(entity_id);
    }
}

fn main() -> Result<(), laminar::ErrorKind> {    
    println!("Starting server..");
    init()
}

struct ClientMapper(HashMap<SocketAddr, EntityId>);

impl ClientMapper {
    fn new() -> Self {
        Self(HashMap::new())
    }

    fn insert(&mut self, addr: SocketAddr, entity_id: EntityId) {
        self.0.insert(addr, entity_id);
    }

    fn remove(&mut self, addr: &SocketAddr) -> Option<EntityId> {
        self.0.remove(addr)
    }

    fn get(&self, addr: &SocketAddr) -> Option<&EntityId> {
        self.0.get(addr)
    } 
}
