use std::time::{self, Instant, Duration};
use std::collections::HashMap;
use std::env;

extern crate piston_window;

use piston_window::*;
use laminar::{ErrorKind, Packet, Socket, SocketEvent, Config};
use shipyard::*;

use netcarrier::{Game, NetworkState, Position, NetworkIdentifier, ClientState};

const SERVER: &str = "127.0.0.1:12351";

#[allow(unreachable_code)]
pub fn init(addr: &str) -> Result<(), ErrorKind> {
	let mut config = Config::default();
	config.heartbeat_interval = Some(Duration::from_secs(1));
    let mut socket = Socket::bind_with_config(addr, config)?;
    println!("Connected on {}", addr);
	
	let mut net_id_mapping: HashMap<usize, EntityId> = HashMap::new();

    let game = Game::new_empty();

    let server = SERVER.parse().unwrap();

    let mut client_state = ClientState::default();

    socket.send(Packet::reliable_unordered(
        server,
        "connect".as_bytes().to_vec(),
    ))?;

    let mut window: PistonWindow =
        WindowSettings::new("Hello Piston!", [640, 480])
        .exit_on_esc(true)
        .build().unwrap();
    window.set_max_fps(20);
    window.set_ups(20);
    while let Some(event) = window.next() {
        window.draw_2d(&event, |context, graphics, _device| {
            clear([1.0; 4], graphics);
            game.world.run(|positions: View<Position>, rectangles: View<Rectangle>| {
                (&positions, &rectangles).iter().for_each(|(pos, rec)| {
                    rectangle([1.0, 0.0, 0.0, 1.0], // red
                              [pos.x as f64, pos.y as f64, rec.width as f64, rec.height as f64],
                              context.transform,
                              graphics);
                });
            })    
        });

		let start = time::Instant::now();

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
		socket.send(Packet::unreliable(
            server,
            encoded_client,
        ))?;

        socket.manual_poll(Instant::now());

        match socket.recv() {
            Some(SocketEvent::Packet(packet)) => {
                if packet.addr() == server {
                    // println!("Server sent: {}", String::from_utf8_lossy(packet.payload()));
                    println!("Server sent a packet");
                    let msg = packet.payload();
					let decoded: NetworkState = bincode::deserialize(msg).unwrap();
					run(&game.world, &decoded, &mut net_id_mapping);

					// game.world.run::<&Position, _, _>(|positions| {
        			// 	positions.iter().for_each(|pos| {
                    //     });
    			    // });
                } else {
                    println!("Unknown sender.");
                }
            }
            Some(SocketEvent::Timeout(_)) => {},
            _ => ()
		}
    }

    Ok(())
}

fn run(world: &World, net_state: &NetworkState, net_id_mapping: &mut HashMap<usize, EntityId>) {
    world.run(|mut entities: EntitiesViewMut, mut positions: ViewMut<Position>, mut rectangles: ViewMut<Rectangle>| {
        for (pos, net_id) in &net_state.positions {
            if let Some(&id) = net_id_mapping.get(net_id) {
                positions[id] = *pos;
            } else {
                let entity = entities.add_entity((&mut positions, &mut rectangles), (*pos, Rectangle::new(100.0, 100.0)));
                net_id_mapping.insert(*net_id, entity);
            }
        }
    });
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
