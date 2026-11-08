use musicbox_core::Track;
use musicbox_core::tracks::drone::Drone;
use musicbox_core::tracks::ambient_techno::AmbientTechno;
use musicbox_core::tuning::{scale_by_id, Intonation, Tuning};

fn no_entropy(_: &mut [u8]) -> Result<(), getrandom::Error> { Err(getrandom::Error::UNSUPPORTED) }
getrandom::register_custom_getrandom!(no_entropy);

enum Engine { Drone(Drone), Techno(AmbientTechno) }

struct State { engine: Engine, l: Vec<f32>, r: Vec<f32> }

static mut STATE: Option<State> = None;
static mut NAME: [u8; 64] = [0; 64];

#[allow(static_mut_refs)]
fn st() -> &'static mut State { unsafe { STATE.as_mut().unwrap() } }

#[unsafe(no_mangle)]
/// root: semitones from A (-5..=6); scale: 0 = original, see `scale_by_id`;
/// reference: A4 in Hz; intonation: 0 equal, 1 just (5-limit), 2 just (7-limit).
pub extern "C" fn mb_init(kind: u32, sample_rate: u32, seed_hi: u32, seed_lo: u32,
                          root: i32, scale: u32, reference: f32, intonation: u32) {
    let tuning = Tuning {
        root: root.clamp(-5, 6),
        scale: scale_by_id(scale),
        reference: if reference > 300.0 && reference < 600.0 { reference } else { 440.0 },
        intonation: match intonation { 1 => Intonation::Just5, 2 => Intonation::Just7, _ => Intonation::Equal },
    };
    let seed = ((seed_hi as u64) << 32) | seed_lo as u64;
    let engine = if kind == 1 { Engine::Techno(AmbientTechno::with_tuning(sample_rate, seed, tuning)) }
                 else { Engine::Drone(Drone::with_tuning(sample_rate, seed, tuning)) };
    unsafe { STATE = Some(State { engine, l: vec![0.0; 4096], r: vec![0.0; 4096] }); }
}

#[unsafe(no_mangle)]
pub extern "C" fn mb_render(frames: usize) {
    let s = st();
    let f = frames.min(4096);
    match &mut s.engine {
        Engine::Drone(e) => e.render(&mut s.l[..f], &mut s.r[..f]),
        Engine::Techno(e) => e.render(&mut s.l[..f], &mut s.r[..f]),
    }
}

#[unsafe(no_mangle)] pub extern "C" fn mb_left() -> *const f32 { st().l.as_ptr() }
#[unsafe(no_mangle)] pub extern "C" fn mb_right() -> *const f32 { st().r.as_ptr() }

#[unsafe(no_mangle)]
pub extern "C" fn mb_fade_out() {
    match &mut st().engine { Engine::Drone(e) => e.start_fade_out(), Engine::Techno(e) => e.start_fade_out() }
}

#[unsafe(no_mangle)]
pub extern "C" fn mb_is_done() -> u32 {
    (match &st().engine { Engine::Drone(e) => e.is_done(), Engine::Techno(e) => e.is_done() }) as u32
}

#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn mb_name_buf() -> *mut u8 { unsafe { NAME.as_mut_ptr() } }

#[unsafe(no_mangle)]
#[allow(static_mut_refs)]
pub extern "C" fn mb_set_param(len: usize, value: f32) {
    let bytes = unsafe { &NAME[..len.min(64)] };
    if let Ok(name) = std::str::from_utf8(bytes) {
        match &mut st().engine {
            Engine::Techno(e) => e.set_param(name, value),
            Engine::Drone(e) => e.set_param(name, value),
        }
    }
}

/// Sounding notes: 12 values (0..1) by semitone above the key root.
#[unsafe(no_mangle)]
pub extern "C" fn mb_notes() -> *const f32 {
    match &st().engine { Engine::Drone(e) => e.notes().as_ptr(), Engine::Techno(e) => e.notes().as_ptr() }
}

/// Drone layer peak levels (bass, mid, pluck, high); null for techno.
#[unsafe(no_mangle)]
pub extern "C" fn mb_levels() -> *const f32 {
    match &st().engine { Engine::Drone(e) => e.levels().as_ptr(), Engine::Techno(_) => std::ptr::null() }
}
