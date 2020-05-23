use std::env;
use std::net::SocketAddr;

extern crate piston_window;

use laminar::ErrorKind;
use piston_window::*;
use shipyard::*;

use netcarrier::shared::{ClientState, Color, NetworkState, Position, Rectangle};
use netcarrier::transport::{self, JitBuffer, Message, TransportResource};

const SERVER: &str = "127.0.0.1:12351";


#[allow(unreachable_code)]
pub fn init(addr: &str) -> Result<(), ErrorKind> {
    println!("Connected on {}", addr);
    let mut world = World::default();
    transport::init_client_network::<NetworkState>(&mut world, addr, SERVER)?;
    let server: SocketAddr = SERVER.parse().unwrap();
    let mut client_state = ClientState::default();

    let mut window: PistonWindow = WindowSettings::new("Hello Piston!", [640, 480])
        .exit_on_esc(true)
        .build()
        .unwrap();
    window.set_max_fps(20);
    window.set_ups(20);
    while let Some(event) = window.next() {
        window.draw_2d(&event, |context, graphics, _device| {
            clear([1.0; 4], graphics);
            world.run(
                |positions: View<Position>, rectangles: View<Rectangle>, colors: View<Color>| {
                    (&positions, &rectangles, &colors)
                        .iter()
                        .for_each(|(pos, rec, color)| {
                            rectangle(
                                color.0,
                                [
                                    pos.x as f64,
                                    pos.y as f64,
                                    rec.width as f64,
                                    rec.height as f64,
                                ],
                                context.transform,
                                graphics,
                            );
                        });
                },
            )
        });

        if let Some(Button::Keyboard(key)) = event.press_args() {
            match key {
                Key::A => client_state.left = true,
                Key::D => client_state.right = true,
                Key::W => client_state.up = true,
                Key::S => client_state.down = true,
                _ => (),
            }
        };
        if let Some(Button::Keyboard(key)) = event.release_args() {
            match key {
                Key::A => client_state.left = false,
                Key::D => client_state.right = false,
                Key::W => client_state.up = false,
                Key::S => client_state.down = false,
                _ => (),
            }
        };
        println!("{:?}", client_state);
        let encoded_client = bincode::serialize(&client_state).unwrap();
        // TODO: encode client as unique component
        // TODO: review how to expose the network systems
        world.run(|mut transport: UniqueViewMut<TransportResource>| {
            transport
                .messages
                .push_back(Message::new(vec![server], &encoded_client));
        });
        world.run(process_events);
        world.run(transport::send_network_system);
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

// TODO: this should be on our API side
fn process_events(all_storages: AllStoragesViewMut) {
    let net_state: Option<NetworkState>;
    {
        let jit_buffer = all_storages.borrow::<UniqueViewMut<JitBuffer<NetworkState>>>();
        let mut jit_buffer = jit_buffer.0.lock().unwrap();
        println!("JitBuffer: {:?}", jit_buffer.len());
        // TODO: review how to define the size of the jit buffer (config, dynamic...)
        if jit_buffer.len() < 3 {
            return;
        }
        net_state = jit_buffer.pop();
    }
    if let Some(state) = net_state {
        state.apply_state(all_storages);
    }
}
