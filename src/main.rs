// Namespace imports
use std::{sync::{atomic::{AtomicI32, Ordering}, Arc}, thread::{sleep, yield_now}, time::{Duration, Instant}};

use sdl3::{
    audio::{AudioCallback, AudioFormat, AudioSpec, AudioStream},
    event::{DisplayEvent, Event, WindowEvent},
    hint::names::RENDER_VSYNC, keyboard::Keycode,
    pixels::{Color, PixelFormat, PixelMasks},
    sys::render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE, video::Display
};

use crate::chip8::Chip8;

// #![windows_subsystem = "windows"]
extern crate sdl3;
mod chip8;

// Constants
const SDL3_CHIP8_KEY_MAP: [Keycode; 16] = [
    Keycode::X, Keycode::_1, Keycode::_2, Keycode::_3, Keycode::Q, Keycode::W, Keycode::E, Keycode::A,
    Keycode::S, Keycode::D, Keycode::Z, Keycode::C, Keycode::_4, Keycode::R, Keycode::F, Keycode::V,
];
    
const NANOS_IN_SECOND: i64 = 1000000000;
const CONSOLE_MESSAGES: bool = false;

const BACKGROUND_COLOR: u32 = 0xFF000000;
const FOREGROUND_COLOR: u32 = 0xFFFFFFFF;

// Allows convenient error handling by returning a message
fn main() {
    if let Some(message) = app_main() {
        println!("{message}");
    }
}

fn app_main() -> Option<&'static str> {
    // Batches sdl3 objects out of a struct
    let sdl_context = match sdl3::init() {
        Ok(context) => context,
        Err(_) => return Some("Failed to initialize SDL3!")
    };

    // Initializes SDL3 subsystems
    let sdl_audio_subsystem = match sdl_context.audio() {
        Ok(audio) => audio,
        Err(_) => return Some("Failed to initialize audio subsystem!")
    };
    
    let mut sdl_event_pump = match sdl_context.event_pump() {
        Ok(pump) => pump,
        Err(_) => return Some("Failed to initialize event pump!")
    };

    let sdl_video_subsystem = match sdl_context.video() {
        Ok(video) => video,
        Err(_) => return Some("Failed to initialize video subsystem!")
    };

    // Initializes window and renderer
    let sdl_window = match sdl_video_subsystem.window("chip8-emulator", 16 << 6, 9 << 6)
        .resizable().build() {
            Ok(window) => window,
            Err(_) => return Some("Failed to initialize window!")
    };
    
    // Initializes window's display to get refresh rate
    let mut sdl_display = match sdl_window.get_display() {
        Ok(display) => display,
        Err(_) => return Some("Failed to get window's display!")
    };

    // Enables vsync and sets rendering size to 64x32
    sdl3::hint::set(RENDER_VSYNC, "1");
    let mut sdl_canvas = sdl_window.into_canvas();
    if sdl_canvas.set_logical_size(64, 32, SDL_LOGICAL_PRESENTATION_INTEGER_SCALE).is_err() {
        return Some("Failed to set logical size!")
    }

    // Sets the rendering background color
    let agrb8888 = PixelMasks{bpp: 32, rmask: 0x00FF0000, gmask: 0x0000FF00, bmask: 0x000000FF, amask: 0xFF000000};
    let pixel_format = PixelFormat::from_masks(agrb8888);
    sdl_canvas.set_draw_color(Color::from_u32(&pixel_format, BACKGROUND_COLOR));

    // Initializes audio stream with callback
    let audio_spec = AudioSpec{freq: Some(48000), channels: Some(1), format: Some(AudioFormat::s16_sys())};
    let remaining_samples = Arc::new(AtomicI32::new(0));
    let sdl_audio_stream = match sdl_audio_subsystem.default_playback_device()
        .open_playback_stream_with_callback(&audio_spec, AudioState{buffer: Vec::new(), phase: 0, previous: 0,
        remaining_samples: remaining_samples.clone()}) {
        Ok(stream) => stream,
        Err(_) => return Some("Failed to initialize audio stream!")
    };

    // Starts audio steam
    if sdl_audio_stream.resume().is_err() {
        return Some("Failed to resume audio stream!")
    }

    // Initializes texture on the gpu to blit to
    let texture_creator = sdl_canvas.texture_creator();
    let mut sdl_texture = match texture_creator.create_texture_streaming(pixel_format, 64, 32) {
        Ok(texture) => texture,
        Err(_) => return Some("Failed to initialize texture!")
    };
    sdl_texture.set_scale_mode(sdl3::render::ScaleMode::Nearest);

    // Gets refresh rate from primary display
    let mut refresh_time_nanos = match sdl3_get_refresh_time(sdl_display) {
        Some(time) => time,
        None => return None
    };

    // Initializes the chip8 emulation context
    let mut chip8_context =  match Chip8::init() {
        Ok(context) => context,
        Err(msg) => return Some(msg)
    };

    // Frame timing variables
    let mut is_vsync = true;
    let mut start_time = Instant::now();
    let mut frame_delta = 0;
    let mut frame_delta_buffer = 0;

    // Rendering variables
    let mut previous_sound_timer = 0;
    let mut pixel_buffer = [0; 64 * 32 * 4];

    loop {
        // Event loop
        for event in sdl_event_pump.poll_iter() {
            match event {
                // Quits application and reads keyboard
                Event::Quit {..} => return None,
                Event::KeyDown{keycode: Some(sdl_key), ..} => {
                    for chip8_key in 0..SDL3_CHIP8_KEY_MAP.len() {
                        if sdl_key == SDL3_CHIP8_KEY_MAP[chip8_key] {
                            chip8_context.keyboard[chip8_key] = 1;
                        }
                    }
                },
                Event::KeyUp{keycode: Some(sdl_key), ..} => {
                    for chip8_key in 0..SDL3_CHIP8_KEY_MAP.len() {
                        if sdl_key == SDL3_CHIP8_KEY_MAP[chip8_key] {
                            chip8_context.keyboard[chip8_key] = 0;
                        }
                    }
                },

                // Changes display and recalculates refresh rate when moved
                Event::Window {win_event, ..} => {
                    if let WindowEvent::Moved(..) = win_event {
                        sdl_display = match sdl_canvas.window().get_display() {
                            Ok(display) => display,
                            Err(_) => return Some("Failed to get window's display!")
                        };
                        refresh_time_nanos = match sdl3_get_refresh_time(sdl_display) {
                            Some(time) => time,
                            None => return None
                        };
                    }
                },

                // Recalculates refresh rate when display mode changes
                Event::Display {display, display_event, ..} => {
                    if display == sdl_display && display_event == DisplayEvent::CurrentModeChanged {
                        refresh_time_nanos = match sdl3_get_refresh_time(sdl_display) {
                            Some(time) => time,
                            None => return None
                        };
                    }
                }
                _ => ()
            }
        }

        // https://www.gafferongames.com/post/fix_your_timestep/
        const UPDATE_DELTA: i64 = NANOS_IN_SECOND / 60;
        while frame_delta > UPDATE_DELTA {
            frame_delta -= UPDATE_DELTA;

            // Emulates chip8 for 1/60th of a second
            if let Some(message) = chip8_context.run() {
                println!("{message}");
                return None
            }

            // Sets the remaining samples when the sound timer changes
            if previous_sound_timer + 1 != chip8_context.sound_timer {
                remaining_samples.store(chip8_context.sound_timer as i32 * 48000 / 60, Ordering::Release);    
            }
            previous_sound_timer = chip8_context.sound_timer;
            
            // Updates pixel buffer with frame buffer
            // texture.with_lock(None, |pixel_data, pitch| {});
            for (pixel, state) in pixel_buffer.chunks_exact_mut(4)
                .zip(chip8_context.frame_buffer.iter()) {
                    let color = match state {
                        0 => BACKGROUND_COLOR,
                        _ => FOREGROUND_COLOR
                    };
                    pixel.copy_from_slice(&color.to_le_bytes());
            }
        }

        // let pixel_buffer_u8 = from_raw_parts_mut(pixel_buffer.as_mut_ptr().cast(), pixel_buffer.len() * 4);
        if sdl_texture.update(None, &pixel_buffer, 64 * 4).is_err() {
            return Some("Failed to update texture!")
        }

        // Clear background and copies texture to renderer
        sdl_canvas.clear();
        if sdl_canvas.copy(&mut sdl_texture, None, None).is_err() {
            return Some("Failed to copy texture!")
        };

        // Times the frame and the presentation to the gpu
        let present_time = Instant::now();
        sdl_canvas.present();
        let present_elapsed = present_time.elapsed();
        let mut elapsed_time = start_time.elapsed().as_nanos() as i64;

        // Sets frame delta to the next vsync interval or sleeps remaining frame time
        frame_delta += match is_vsync {
            true => {
                start_time = Instant::now();

                // https://frankforce.com/frame-rate-delta-buffering/
                frame_delta_buffer += elapsed_time;
                let delta = match frame_delta_buffer / refresh_time_nanos {
                    ..-1 => {
                        // Turns off vsync if updating more than one frame ahead
                        if CONSOLE_MESSAGES && is_vsync { println!("Turning off vsync"); }
                        is_vsync = false;
                        elapsed_time
                    }
                    -1 | 0 => refresh_time_nanos,
                    frames => {
                        // Missed at least one frame
                        if CONSOLE_MESSAGES {
                            let missed_frame_count = frame_delta_buffer as f32 / refresh_time_nanos as f32;
                            println!("Missed a vsync by {} frames", missed_frame_count - 1.0);
                        }
                        (frames + 1) * refresh_time_nanos
                    }
                };

                frame_delta_buffer -= delta;
                delta
            } false => {
                // https://blog.bearcats.nl/perfect-sleep-function/
                if elapsed_time < refresh_time_nanos {
                    // Sleeps to minimize spinlock
                    const SLEEP_PERIOD: u64 = 1020000;
                    let mut sleep_time = (refresh_time_nanos - elapsed_time) as u64;
                    if sleep_time > SLEEP_PERIOD {
                        // Subtracts 1.02 ms because of sleep inaccuracy
                        sleep_time -= SLEEP_PERIOD;
                        sleep(Duration::from_nanos(sleep_time));
                    }

                    // Spin-locks the rest remaining period
                    loop {
                        elapsed_time = start_time.elapsed().as_nanos() as i64;
                        if elapsed_time >= refresh_time_nanos { break }
                        yield_now();
                    }

                    // Debug message when an extra 200 microseconds is slept
                    if CONSOLE_MESSAGES && elapsed_time > refresh_time_nanos + 200000 {
                        println!("Slept for an extra {} nanoseconds", elapsed_time - refresh_time_nanos);
                    }
                }

                // Begins frame at the end of sleep
                start_time = Instant::now();
                elapsed_time
            }
        };

        // Turns on vsync if a present call takes more than a frame
        if !is_vsync && present_elapsed.as_nanos() as i64 > refresh_time_nanos {
            if CONSOLE_MESSAGES { println!("Turning on vsync"); }
            frame_delta_buffer = 0;
            is_vsync = true;
        }

        // Caps frame delta in case of very long (10 ms) delay
        if frame_delta > NANOS_IN_SECOND / 10 { frame_delta = NANOS_IN_SECOND / 10 }
    }
}

// Audio callback rendering a filtered square wave
struct AudioState {
    buffer: Vec<i16>,
    phase: u16,
    previous: i16,
    remaining_samples: Arc<AtomicI32>
}

impl AudioCallback<i16> for AudioState {
    fn callback(&mut self, stream: &mut AudioStream, mut requested: i32) {
        // Caps the played samples at the remaining samples
        let remaining_samples = self.remaining_samples.fetch_sub(requested, Ordering::AcqRel);
        if remaining_samples < requested { requested = remaining_samples; }

        // Sets buffer length to zero for next iteration
        self.buffer.clear();

        for _ in 0..requested {
            // Calculates sample from square wave phase
            const HALF_PERIOD_SAMPLES: u16 = (48000.0 / (261.63 * 2.0)) as u16;
            const VOLUME: i16 = 1024;
            let square = match self.phase < HALF_PERIOD_SAMPLES {
                true => VOLUME,
                false => -VOLUME,
            };

            // Blends the previous sample with a square wave
            self.previous = (self.previous as f32 * 0.6) as i16 + square;
            self.buffer.push(self.previous);
            self.phase = (self.phase + 1) % (HALF_PERIOD_SAMPLES * 2);
        }

        // Copies audio samples from a buffer to the audio stream
        if stream.put_data_i16(&self.buffer).is_err() {
            panic!("Failed to fill audio stream!")
        }
    }
}

// Returns frame time of a sdl display in nanoseconds
fn sdl3_get_refresh_time(display: Display) -> Option<i64> {
    let display_mode = match display.get_mode() {
        Ok(mode) => mode,
        Err(_) => {
            println!("Failed to get display mode!");
            return None
        }
    };
    Some((NANOS_IN_SECOND as f32 / display_mode.refresh_rate) as i64)
}
