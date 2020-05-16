use std::time::{self, Instant, Duration};
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;

extern crate piston_window;

use piston_window::*;
use laminar::{ErrorKind, Packet, Socket, SocketEvent, Config};
use shipyard::*;

use netcarrier::{Game, NetworkState, Position, NetworkIdentifier, ClientState, Color};
use netcarrier::transport::{self, TransportResource, Message, EventQueue, NetworkEvent};

const SERVER: &str = "127.0.0.1:12351";

struct NetIdMapping(HashMap<u32, EntityId>);

#[allow(unreachable_code)]
pub fn init(addr: &str) -> Result<(), ErrorKind> {
	// let mut config = Config::default();
	// config.heartbeat_interval = Some(Duration::from_secs(1));
    println!("Connected on {}", addr);
    
	let mut net_id_mapping = NetIdMapping(HashMap::new());
    
    let mut game = Game::new_empty();
    game.world.add_unique(net_id_mapping);
    transport::init_client_network(&mut game.world, addr, SERVER)?;
    let server: SocketAddr = SERVER.parse().unwrap();
    
    let mut client_state = ClientState::default();

    let mut window: PistonWindow =
        WindowSettings::new("Hello Piston!", [640, 480])
        .exit_on_esc(true)
        .build().unwrap();
    window.set_max_fps(20);
    window.set_ups(20);
    while let Some(event) = window.next() {
        window.draw_2d(&event, |context, graphics, _device| {
            clear([1.0; 4], graphics);
            game.world.run(|positions: View<Position>, rectangles: View<Rectangle>, colors: View<Color>| {
                (&positions, &rectangles, &colors).iter().for_each(|(pos, rec, color)| {
                    rectangle(color.0,
                              [pos.x as f64, pos.y as f64, rec.width as f64, rec.height as f64],
                              context.transform,
                              graphics);
                });
            })    
        });

        if let Some(Button::Keyboard(key)) = event.press_args() {
            match key {
                Key::A => client_state.left = true,
                Key::D => client_state.right = true,
                Key::W => client_state.up = true,
                Key::S => client_state.down = true,
                _ => ()
            }
        };
        if let Some(Button::Keyboard(key)) = event.release_args() {
            match key {
                Key::A => client_state.left = false,
                Key::D => client_state.right = false,
                Key::W => client_state.up = false,
                Key::S => client_state.down = false,
                _ => ()
            }
        };
        println!("{:?}", client_state);
        let encoded_client = bincode::serialize(&client_state).unwrap();
        // TODO: encode client as unique component
        game.world.run(|mut transport: UniqueViewMut<TransportResource>| {
            transport.messages.push_back(Message::new(vec![server], &encoded_client));
        });
        game.world.run(process_events);
        game.world.run(transport::send_network_system);
    }

    Ok(())
}

fn main() -> Result<(), laminar::ErrorKind> {    
    println!("Starting client..");
    
    let mut args = env::args();
    args.next();
    let addr = match args.next() {
        Some(arg) => arg,
        None => "127.0.0.1:12352".to_string(),
    };

    println!("address: {}", addr);
    
    init(&addr)
}

struct Rectangle {
    width: f32,
    height: f32,
}

impl Rectangle {
    fn new(width: f32, height: f32) -> Rectangle {
        Rectangle { width, height }
    }
}

fn process_events(mut entities: EntitiesViewMut, mut positions: ViewMut<Position>,  mut colors: ViewMut<Color>, mut rectangles: ViewMut<Rectangle>, event_queue: UniqueViewMut<EventQueue>, mut net_id_mapping: UniqueViewMut<NetIdMapping>) {
    println!("EventQueue: {:?}",  event_queue.events.len());
    // for event in &event_queue.events {
    while let Ok(event) = event_queue.events.pop() {
        match event {
            NetworkEvent::Message(addr, bytes) => {
                if let Ok(net_state) = bincode::deserialize::<NetworkState>(&bytes) {
                    println!("Received {:?} from {}.", net_state, addr);
                    let masked_entities_ids = net_state.positions.masked_entities_id(&net_state.entities_id);
                    
                    for (i, pos) in net_state.positions.values.iter().enumerate() {
                        let net_id = &masked_entities_ids[i];
                        if let Some(&id) = net_id_mapping.0.get(net_id) {
                            positions[id] = *pos;
                        } else {
                            let entity = entities.add_entity((&mut positions, &mut rectangles), (*pos, Rectangle::new(20.0, 20.0)));
                            net_id_mapping.0.insert(*net_id, entity);
                        }
                    }
                    let color_masked_entities_ids = net_state.colors.masked_entities_id(&net_state.entities_id);
                    for (i, color) in net_state.colors.values.iter().enumerate() {
                        let net_id = &color_masked_entities_ids[i];
                        if let Some(&id) = net_id_mapping.0.get(net_id) {
                            if !colors.contains(id) {
                                entities.add_component(&mut colors, *color, id);
                            } else {
                                colors[id] = *color;
                            }
                        }
                    }
                }
            },
            _ => {}
        }
    }
    // TODO: use removed array
    // for entity_id in removed_entities {
    //     all_storages.delete(entity_id);
    // }
}
