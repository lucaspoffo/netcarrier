use std::collections::HashMap;
use std::net::SocketAddr;
use std::{thread, time};

use laminar::ErrorKind;
use shipyard::*;

use netcarrier::shared::{ClientState, Color, NetworkState, Position, Rectangle, Velocity};
use netcarrier::transport::{
    self, ClientList, EventList, Message, NetworkEvent, TransportResource,
};
use netcarrier::{NetworkController, NetworkIdentifier};

const MS_PER_FRAME: u64 = 50;
const SERVER: &str = "127.0.0.1:12351";

struct GameState(Vec<u8>);

#[allow(unreachable_code)]
pub fn init() -> Result<(), ErrorKind> {
    let mut world = World::default();
    let mut net_controller = NetworkController::default();
    transport::init_network(&mut world, SERVER)?;
    world.add_unique(GameState(vec![]));
    world.add_unique(ClientMapper::default());

    loop {
        net_controller.tick();
        println!("frame: {}", net_controller.frame);

        let start = time::Instant::now();
        world.run(process_events);
        world.run(system_update_player);
        world.run(system_move);

        let net_state = NetworkState::new(&world, net_controller.frame);
        let game_encoded: Vec<u8> = bincode::serialize(&net_state).unwrap();

        world.run(|mut game_state: UniqueViewMut<GameState>| {
            println!("GameState: {:?}", game_encoded);
            game_state.0 = game_encoded;
        });

        // TODO: this should not be run manually in the server, expose a better API
        world.run(add_players_packets);
        world.run(transport::send_network_system);

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

fn add_players_packets(
    client_list: UniqueView<ClientList>,
    mut transport: UniqueViewMut<TransportResource>,
    game_state: UniqueView<GameState>,
) {
    transport.messages.push_back(Message::new(
        client_list.clients.lock().unwrap().clone(),
        &game_state.0[..],
    ));
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
        let event_list = all_storages.borrow::<UniqueViewMut<EventList>>();
        let mut client_mapper = all_storages.borrow::<UniqueViewMut<ClientMapper>>();
        let mut positions = all_storages.borrow::<ViewMut<Position>>();
        let mut colors = all_storages.borrow::<ViewMut<Color>>();
        let mut rectangles = all_storages.borrow::<ViewMut<Rectangle>>();
        let mut velocities = all_storages.borrow::<ViewMut<Velocity>>();
        let mut clients_state = all_storages.borrow::<ViewMut<ClientState>>();
        let mut net_ids = all_storages.borrow::<ViewMut<NetworkIdentifier>>();

        // TODO: add a cleaner interface to use the event_list
        let mut event_list = event_list.0.lock().unwrap();
        println!("EventList: {:?}", event_list.len());
        event_list.drain(..).for_each(|event| {
            match event {
                NetworkEvent::Connect(addr) => {
                    println!("Client {} connected.", addr);
                    if client_mapper.get(&addr).is_some() {
                        return;
                    }

                    let net_id = NetworkIdentifier::default();
                    let entity = entities.add_entity(
                        (
                            &mut positions,
                            &mut velocities,
                            &mut net_ids,
                            &mut clients_state,
                            &mut colors,
                            &mut rectangles,
                        ),
                        (
                            Position::new(100.0, 100.0),
                            Velocity::new(0.0, 0.0),
                            net_id,
                            ClientState::default(),
                            Color::random(),
                            Rectangle::new(20.0, 20.0),
                        ),
                    );
                    client_mapper.insert(addr.clone(), entity);
                }
                NetworkEvent::Disconnect(addr) => {
                    println!("Client {} disconnected.", addr);
                    if let Some(entity_id) = client_mapper.remove(&addr) {
                        removed_entities.push(entity_id);
                    }
                }
                NetworkEvent::Message(addr, bytes) => {
                    // TODO: Review how to treat client state
                    if let Ok(decoded) = bincode::deserialize::<ClientState>(&bytes) {
                        println!("Received {:?} from {}.", decoded, addr);
                        if let Some(entity_id) = client_mapper.get(&addr) {
                            clients_state[*entity_id] = decoded;
                        }
                    }
                }
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

#[derive(Default)]
struct ClientMapper(HashMap<SocketAddr, EntityId>);

impl ClientMapper {
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
