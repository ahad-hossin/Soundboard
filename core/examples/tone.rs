//! Lists output devices and plays a test tone.
//! cargo run -p soundboard-core --example tone            -> default device
//! cargo run -p soundboard-core --example tone "CABLE Input (VB-Audio Virtual Cable)"
use soundboard_core::audio::{sine, AudioBackend, DeviceTarget, RodioBackend};

fn main() {
    let backend = RodioBackend::new();
    for d in backend.devices() {
        println!("{}{}", d.name, if d.is_default { "  (default)" } else { "" });
    }
    let target = DeviceTarget::from_name(std::env::args().nth(1).as_deref());
    println!("playing on {target:?}");
    let h = backend.play(&target, &sine(440.0, 0.6, 44100), 0.3).expect("play");
    while !backend.is_finished(h) {
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}
