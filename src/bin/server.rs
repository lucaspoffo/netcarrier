use std::{thread, time::{self, Duration}};
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

use laminar::{ErrorKind, Packet, Socket, SocketEvent};
use shipyard::*;

use netcarrier::{Game, Position, Velocity, NetworkIdentifier, ClientState};

const MS_PER_FRAME: u64 = 50;
const SERVER: &str = "127.0.0.1:12351";

#[allow(unreachable_code)]
pub fn init() -> Result<(), ErrorKind> {
    let mut socket = Socket::bind(SERVER)?;
    let (sender, receiver) = (socket.get_packet_sender(), socket.get_event_receiver());
    let _thread = thread::spawn(move || socket.start_polling());
    
    let mut game = Game::new_empty();

    let mut players_state: Arc<Mutex<HashMap<SocketAddr, ClientState>>> = Arc::new(Mutex::new(HashMap::new()));
    
    let clients: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(vec![]));
    let clients_added: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(vec![]));
    let clients_removed: Arc<Mutex<Vec<SocketAddr>>> = Arc::new(Mutex::new(vec![]));
    let pool_clients = clients.clone();
    let clone_players_state = players_state.clone();
    let clone_clients_added = clients_added.clone();
    let clone_clients_removed = clients_removed.clone();
    let _pool = thread::spawn(move || {
        loop {
            if let Ok(event) = receiver.recv() {
                match event {
                    SocketEvent::Packet(packet) => {
                        let msg = packet.payload();
                        let ip = packet.addr().ip();
                        let mut c = pool_clients.lock().unwrap();
                        if !c.contains(&packet.addr()) {
                            let mut ca = clone_clients_added.lock().unwrap();
                            c.push(packet.addr());
                            ca.push(packet.addr());
                        }
                        // let decoded: ClientState = bincode::deserialize(msg).unwrap();
                        // println!("Received {:?} from {:?}", decoded, ip);
                        // clone_players_state.lock().unwrap().insert(packet.addr(), decoded);
                        if let Ok(decoded) = bincode::deserialize::<ClientState>(msg) {
                            // println!("Received {:?} from {:?}", decoded, ip);
                            clone_players_state.lock().unwrap().insert(packet.addr(), decoded);
                        }
                    }
                    SocketEvent::Timeout(address) => {
                        let mut c = pool_clients.lock().unwrap();

                        c.retain(|&x| x != address);
                        let mut cr = clone_clients_removed.lock().unwrap();
                        cr.push(address);
                        println!("Client timed out: {}", address);
                    },
                    SocketEvent::Connect(addres) => {
                        // let mut c = pool_clients.lock().unwrap();
                        // let mut ca = clone_clients_added.lock().unwrap();
                        // c.push(addres);
                        // ca.push(addres);
                        println!("Address {} connected.", addres);
                    }
                }
            }
        }
    });

    let mut spawn_time = Duration::from_secs(2);

    loop {
        let start = time::Instant::now();
        let current_players_state = {
            let data = players_state.lock().unwrap();
            data.clone()
        };

        {
            let mut ca = clients_added.lock().unwrap();
            if ca.len() > 0 {
                game.world.run(|mut entities: EntitiesViewMut, mut positions: ViewMut<Position>, mut net_ids: ViewMut<NetworkIdentifier>, mut client_controllers: ViewMut<ClientController>, mut velocities: ViewMut<Velocity>| {
                    for c in ca.iter() {
                        let net_id = NetworkIdentifier::new();
                        entities.add_entity(
                            (&mut positions, &mut velocities, &mut net_ids, &mut client_controllers),
                            (Position::new(100.0, 100.0), Velocity::new(0.0, 0.0), net_id, ClientController::new(c.clone()))
                        );
                    }
                });
                ca.clear();
            }
        }
        
        game.world.run(|mut client_controllers: ViewMut<ClientController>| {
            for controller in (&mut client_controllers).iter() {
                if let Some(state) = current_players_state.get(&controller.addr) {
                    controller.state = state.clone(); 
                }
            }
        });
        game.world.run(system_update_player);
        game.world.run(system_move);
        // game.update();
        
        // println!("spawn time: {:?}", spawn_time);
        // if let Some(x) = spawn_time.checked_sub(Duration::from_millis(MS_PER_FRAME)) {
            //     spawn_time = x;  
            // } else {
                //     spawn_time = Duration::from_secs(2);
                //     game.world.run::<(EntitiesMut, &mut Position, &mut Velocity, &mut NetworkIdentifier), _, _>(|(mut entities, mut positions, mut velocities, mut net_identifiers)| {
                    //         let net_id = NetworkIdentifier::new();
                    //         entities.add_entity(
                        //             (&mut positions, &mut velocities, &mut net_identifiers),
                        //             (Position::new(0.0, 0.0), Velocity::new(1.0, 1.0), net_id)
                        //         );
                        //     });
                        // }
                        
        let encoded: Vec<u8> = game.encoded();
        
        let c = clients.lock().unwrap();
        // println!("clients: {}", c.len());
        for client in &*c {
            // println!("sending for {}", client);
            sender.send(Packet::reliable_unordered(*client, encoded.clone())).expect("This should send");
        }

        let now = time::Instant::now();
        let frame_duration = time::Duration::from_millis(MS_PER_FRAME);
        // let wait = (start + frame_duration) - now;
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
    }
}

fn system_update_player(controllers: View<ClientController>, mut velocities: ViewMut<Velocity>) {
    for (controller, velocity) in (&controllers, &mut velocities).iter() {
        velocity.dx = (controller.state.right as i32 - controller.state.left as i32) as f32;
        velocity.dy = (controller.state.down as i32 - controller.state.up as i32) as f32;
    }
}

fn main() -> Result<(), laminar::ErrorKind> {    
    println!("Starting server..");
    init()
}

#[derive(Debug)]
struct ClientController {
    addr: SocketAddr,
    state: ClientState
}

impl ClientController {
    fn new(addr: SocketAddr) -> Self {
        Self { addr, state: ClientState::default() }
    }
}
