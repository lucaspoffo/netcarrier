use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;

extern crate piston_window;

use piston_window::*;
use laminar::ErrorKind;
use shipyard::*;

use netcarrier::{Game, NetworkState, NetworkIdentifier};
use netcarrier::transport::{self, TransportResource, Message, EventList, NetworkEvent};
use netcarrier::shared::{ClientState, Color, Position, Rectangle};

const SERVER: &str = "127.0.0.1:12351";

struct NetIdMapping(HashMap<u32, EntityId>);

#[allow(unreachable_code)]
pub fn init(addr: &str) -> Result<(), ErrorKind> {
    println!("Connected on {}", addr);
    
    let mut game = Game::new_empty();

	let net_id_mapping = NetIdMapping(HashMap::new());
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
        // TODO: review how to expose the network systems
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

fn process_events(entities: EntitiesViewMut, positions: ViewMut<Position>,  colors: ViewMut<Color>, rectangles: ViewMut<Rectangle>, event_list: UniqueViewMut<EventList>, mut net_id_mapping: UniqueViewMut<NetIdMapping>) {
    let mut event_list = event_list.0.lock().unwrap();
    println!("EventList: {:?}",  event_list.len());
    // TODO: we should have a jit buffer when appling state from the server and removing from it based on frame and removing any state that has lower frame than alredy processed
    if let Some(event) = event_list.pop() {
        match event {
            NetworkEvent::Message(addr, bytes) => {
                if let Ok(net_state) = bincode::deserialize::<NetworkState>(&bytes) {
                    println!("Received {:?} from {}.", net_state, addr);
                    net_state.apply_state(entities, &mut net_id_mapping.0, positions, colors, rectangles);
                }
            },
            _ => {}
        }
    }    
    event_list.clear();
    // TODO: use removed array
    // for entity_id in removed_entities {
    //     all_storages.delete(entity_id);
    // }
}
