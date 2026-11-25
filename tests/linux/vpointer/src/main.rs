//! A virtual mouse for tests under headless Sway, driven from stdin:
//!
//!     move <x> <y>     absolute position in output pixels
//!     click            left button down and up
//!     rclick           right button down and up
//!
//! `wlrctl` creates a virtual pointer per command and removes it right after, so the
//! seat's pointer comes and goes before a client can bind it. This one stays until stdin
//! closes, like a real mouse.

use std::io::BufRead;
use std::time::{Duration, Instant};

use wayland_client::protocol::{wl_registry, wl_seat::WlSeat};
use wayland_client::{Connection, Dispatch, QueueHandle, delegate_noop};
use wayland_protocols_wlr::virtual_pointer::v1::client::{
    zwlr_virtual_pointer_manager_v1::ZwlrVirtualPointerManagerV1,
    zwlr_virtual_pointer_v1::ZwlrVirtualPointerV1,
};

#[derive(Default)]
struct State {
    seat: Option<WlSeat>,
    manager: Option<ZwlrVirtualPointerManagerV1>,
}

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(state: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &(), _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_registry::Event::Global { name, interface, version } = event {
            match interface.as_str() {
                "wl_seat" => state.seat = Some(registry.bind(name, version.min(7), qh, ())),
                "zwlr_virtual_pointer_manager_v1" => state.manager = Some(registry.bind(name, version.min(2), qh, ())),
                _ => {}
            }
        }
    }
}

delegate_noop!(State: ignore WlSeat);
delegate_noop!(State: ZwlrVirtualPointerManagerV1);
delegate_noop!(State: ZwlrVirtualPointerV1);

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;

fn main() {
    let width: u32 = std::env::var("W").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let height: u32 = std::env::var("H").ok().and_then(|v| v.parse().ok()).unwrap_or(1080);
    let conn = Connection::connect_to_env().expect("no Wayland display");
    let mut queue = conn.new_event_queue();
    let qh = queue.handle();
    conn.display().get_registry(&qh, ());
    let mut state = State::default();
    queue.roundtrip(&mut state).unwrap();
    let manager = state.manager.as_ref().expect("compositor has no zwlr_virtual_pointer_manager_v1");
    let pointer = manager.create_virtual_pointer(state.seat.as_ref(), &qh, ());
    queue.roundtrip(&mut state).unwrap();
    eprintln!("vpointer ready");

    let start = Instant::now();
    let now = || start.elapsed().as_millis() as u32;
    for line in std::io::stdin().lock().lines() {
        let line = line.unwrap();
        let parts: Vec<&str> = line.split_whitespace().collect();
        match parts.as_slice() {
            ["move", x, y] => {
                pointer.motion_absolute(now(), x.parse().unwrap(), y.parse().unwrap(), width, height);
                pointer.frame();
            }
            ["click"] | ["rclick"] => {
                let button = if parts[0] == "click" { BTN_LEFT } else { BTN_RIGHT };
                pointer.button(now(), button, wayland_client::protocol::wl_pointer::ButtonState::Pressed);
                pointer.frame();
                conn.flush().unwrap();
                std::thread::sleep(Duration::from_millis(60));
                pointer.button(now(), button, wayland_client::protocol::wl_pointer::ButtonState::Released);
                pointer.frame();
            }
            _ => eprintln!("vpointer: unknown command {line:?}"),
        }
        conn.flush().unwrap();
        queue.roundtrip(&mut state).unwrap();
    }
}
