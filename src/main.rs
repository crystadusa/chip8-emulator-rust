// Namespace imports
use std::{slice::from_raw_parts, sync::{Arc, atomic::{AtomicI32, Ordering}}, thread::{sleep, yield_now}, time::{Duration, Instant}};

use sdl3::{
    audio::{AudioCallback, AudioFormat, AudioSpec, AudioStream},
    event::{DisplayEvent, Event, WindowEvent},
    hint::names::RENDER_VSYNC, keyboard::Keycode,
    pixels::{Color, PixelFormat, PixelMasks},
    render::ScaleMode, sys::{render::SDL_LOGICAL_PRESENTATION_INTEGER_SCALE},
    video::{Display, FullscreenType, WindowPos}
};

// #![windows_subsystem = "windows"]
mod chip8;
mod config;
use crate::{chip8::Chip8, config::Chip8Configuration};
extern crate sdl3;

// Constants
const SDL3_CHIP8_KEY_MAP: [Keycode; 16] = [
    Keycode::X, Keycode::_1, Keycode::_2, Keycode::_3, Keycode::Q, Keycode::W, Keycode::E, Keycode::A,
    Keycode::S, Keycode::D, Keycode::Z, Keycode::C, Keycode::_4, Keycode::R, Keycode::F, Keycode::V,
];
    
const NANOS_IN_SECOND: u64 = 1000000000;
const CONSOLE_MESSAGES: bool = false;

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

    // Initializes window
    let mut sdl_window = match sdl_video_subsystem.window("chip8-emulator", 0, 0)
    .hidden().resizable().build() {
        Ok(window) => window,
        Err(_) => return Some("Failed to initialize window!")
    };

    // Initializes the primary display to get its resolution and refresh rate
    let mut sdl_display = match sdl_video_subsystem.get_primary_display() {
        Ok(display) => display,
        Err(_) => return Some("Failed to get primary display!")
    };

    // Gets configuration for this emulator
    let chip8_configuration = match Chip8Configuration::parse(&sdl_window, &mut sdl_event_pump) {
        Ok(config) => config,
        Err(msg) => match msg {
            "" => return Some(msg),
            _ => {
                println!("{msg}");
                return Some("Run \"chip8-emulator -h\" for more information.")
            }
        }
    };

    // Initializes the chip8 emulation context
    let mut chip8_context =  match Chip8::init(&chip8_configuration) {
        Ok(context) => context,
        Err(msg) => return Some(msg)
    };

    // Sets fullscreen mode from configuration
    if sdl_window.set_fullscreen(chip8_configuration.is_fullscreen).is_err() {
        return Some("Failed to set fullscreen mode!");
    }

    // Enables vsync based on configuration
    if chip8_configuration.is_vsync { sdl3::hint::set(RENDER_VSYNC, "1"); }

    // Calculates window size based on scale factor, pixel dimensions, or half the monitor resolution
    let (window_width, window_height) = match chip8_configuration.window_size {
        None => match sdl_display.get_mode() {
            // Sets the window size to half the highest integer scale
            Ok(mode) => (mode.w as u32 / 64 * 32, mode.h as u32 / 32 * 16),
            Err(_) => return Some("Failed to get display mode!")
        }
        Some(size) => match size {
            // Calculates window size from an integer scale chip8 (64x32) resolution
            Err(scale) => (64 * scale, 32 * scale),
            Ok(size) => size,
        }
    };

    // Sets window size, centers it, and shows it
    if sdl_window.set_size(window_width, window_height).is_err() {
        return Some("Failed to set window size!")
    }
    sdl_window.set_position(WindowPos::Centered, WindowPos::Centered);
    sdl_window.show();

    // Sets rendering size to 64x32
    let mut sdl_canvas = sdl_window.into_canvas();
    if sdl_canvas.set_logical_size(64, 32, SDL_LOGICAL_PRESENTATION_INTEGER_SCALE).is_err() {
        return Some("Failed to set logical size!")
    }

    // Sets the rendering background color
    let agrb8888 = PixelMasks{bpp: 32, rmask: 0x00FF0000, gmask: 0x0000FF00, bmask: 0x000000FF, amask: 0xFF000000};
    let pixel_format = PixelFormat::from_masks(agrb8888);
    sdl_canvas.set_draw_color(Color::from_u32(&pixel_format, chip8_configuration.background_color));

    // Initializes audio stream with callback
    let audio_spec = AudioSpec{freq: Some(48000), channels: Some(1), format: Some(AudioFormat::s16_sys())};
    let sdl_audio_stream = match sdl_audio_subsystem.default_playback_device()
    .open_playback_stream_with_callback(&audio_spec, AudioState{buffer: Vec::new(), phase: 0, previous: 0,
        remaining_samples: chip8_context.remaining_samples.clone()}) {
        Ok(stream) => stream,
        Err(_) => return Some("Failed to initialize audio stream!")
    };

    // Starts audio steam
    if sdl_audio_stream.resume().is_err() {
        return Some("Failed to resume audio stream!")
    }

    // Initializes texture on the gpu to blit to
    let texture_creator = sdl_canvas.texture_creator();
    let mut sdl_texture = match texture_creator.create_texture_streaming(pixel_format,
         chip8::FRAME_BUFFER_WIDTH as u32, chip8::FRAME_BUFFER_HEIGHT as u32) {
        Ok(texture) => texture,
        Err(_) => return Some("Failed to initialize texture!")
    };
    sdl_texture.set_scale_mode(ScaleMode::Nearest);

    // Gets refresh rate from primary display
    let mut refresh_time_nanos = match sdl3_get_refresh_time(sdl_display) {
        Some(time) => time,
        None => return None
    };

    // Frame timing variables
    let mut is_vsync = chip8_configuration.is_vsync;
    let mut start_time = Instant::now();
    let mut frame_delta = 0;
    let mut frame_delta_buffer = 0;

    let mut average_total = 0;
    let mut average_count = 0;

    loop {
        // Event loop
        for event in sdl_event_pump.poll_iter() {
            match event {
                // Quits application and reads keyboard
                Event::Quit {..} => return None,

                Event::KeyDown{keycode: Some(sdl_key), ..} => match sdl_key {
                    // Terminates emulator
                    Keycode::Escape => return None,

                    // Reverses the full screen state
                    Keycode::F11 => {
                        let old_state = sdl_canvas.window().fullscreen_state();
                        if sdl_canvas.window_mut().set_fullscreen(old_state == FullscreenType::Off).is_err() {
                            return Some("Failed to set fullscreen mode!");
                        }
                    }

                    // Handles chip8 key press
                    _ => for chip8_key in 0..SDL3_CHIP8_KEY_MAP.len() {
                        if sdl_key == SDL3_CHIP8_KEY_MAP[chip8_key] {
                            chip8_context.keyboard[chip8_key] = true;
                        }
                    }
                },

                Event::KeyUp{keycode: Some(sdl_key), ..} => {
                    // Handles chip8 key release
                    for chip8_key in 0..SDL3_CHIP8_KEY_MAP.len() {
                        if sdl_key == SDL3_CHIP8_KEY_MAP[chip8_key] {
                            chip8_context.keyboard[chip8_key] = false;
                            chip8_context.key_released[chip8_key] = true;
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

        // Emulates chip8 for the frame time
        let emulation_start = std::time::Instant::now();
        if let Some(message) = chip8_context.run(frame_delta as f32) {
            return Some(message)
        }

        // Displays the average emulation time every 1024 frames
        if CONSOLE_MESSAGES {
            average_total += emulation_start.elapsed().as_nanos();
            average_count += 1;
            if average_count >= 1024 {
                println!("Average emulation frame is {}", average_total / average_count as u128);
                average_total = 0;
                average_count = 0;
            }
        }

        let frame_buffer = chip8_context.frame_buffer.as_slice();
        let pixel_data= unsafe { from_raw_parts(frame_buffer.as_ptr().cast(), chip8::FRAME_BUFFER_SIZE * 4) };
        if sdl_texture.update(None, pixel_data, chip8::FRAME_BUFFER_WIDTH as usize * 4).is_err() {
            return Some("Failed to update texture!")
        }

        // Clear background and copies texture to renderer
        sdl_canvas.clear();
        if sdl_canvas.copy(&mut sdl_texture, None, None).is_err() {
            return Some("Failed to copy texture!")
        };

        // Sets frame delta to the next vsync interval or sleeps remaining frame time
        frame_delta = match is_vsync {
            true => {
                // Presents frame to gpu and gets frame time
                sdl_canvas.present();

                let elapsed_time = start_time.elapsed().as_nanos() as u64;
                start_time = Instant::now();

                // https://frankforce.com/frame-rate-delta-buffering/
                frame_delta_buffer += elapsed_time as i64;
                let delta = match frame_delta_buffer / refresh_time_nanos as i64 {
                    ..-1 => {
                        // Turns off vsync if updating more than one frame ahead
                        if CONSOLE_MESSAGES { println!("Turning off vsync"); }
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
                        (frames as u64 + 1) * refresh_time_nanos
                    }
                };

                frame_delta_buffer -= delta as i64;
                delta
            } false => {
                let mut elapsed_time = start_time.elapsed().as_nanos() as u64;
                if CONSOLE_MESSAGES && elapsed_time >= refresh_time_nanos {
                    println!("Frame took an extra {} nanoseconds", elapsed_time - refresh_time_nanos);
                }

                // https://blog.bearcats.nl/perfect-sleep-function/
                if elapsed_time < refresh_time_nanos {
                    // Sleeps to minimize spinlock
                    const SLEEP_PERIOD: u64 = 1020000;
                    let mut sleep_time = (refresh_time_nanos - elapsed_time) as u64;
                    if sleep_time >= SLEEP_PERIOD {
                        // Subtracts 1.02 ms because of sleep inaccuracy
                        sleep_time -= SLEEP_PERIOD;
                        sleep(Duration::from_nanos(sleep_time));
                    }

                    // Spin-locks the rest remaining period
                    loop {
                        elapsed_time = start_time.elapsed().as_nanos() as u64;
                        if elapsed_time >= refresh_time_nanos { break }
                        yield_now();
                    }

                    // Debug message when an extra 200 microseconds is slept
                    if CONSOLE_MESSAGES && elapsed_time >= refresh_time_nanos + 200000 {
                        println!("Slept for an extra {} nanoseconds", elapsed_time - refresh_time_nanos);
                    }
                }

                // Begins frame with presenting frame to the gpu at the end of sleep
                start_time = Instant::now();
                sdl_canvas.present();
                elapsed_time
            }
        };

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
            println!("Failed to fill audio stream!")
        }
    }
}

// Returns frame time of a sdl display in nanoseconds
fn sdl3_get_refresh_time(display: Display) -> Option<u64> {
    let display_mode = match display.get_mode() {
        Ok(mode) => mode,
        Err(_) => {
            println!("Failed to get display mode!");
            return None
        }
    };
    Some((NANOS_IN_SECOND as f32 / display_mode.refresh_rate) as u64)
}
