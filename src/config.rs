use std::{env, iter::Peekable, path::PathBuf, sync::mpsc, thread::sleep, time::Duration};
use sdl3::{dialog::{show_open_file_dialog, DialogCallback}, EventPump};

pub struct Chip8Configuration {
    pub rom_path: String,
    pub clock_hz: u32,
    pub background_color: u32,
    pub foreground_color: u32,
    pub window_size: Option<Result<(u32 ,u32), u32>>,
    pub is_fullscreen: bool,
    pub is_drawsync: bool,
    pub is_vsync: bool
}

impl Chip8Configuration {
    pub fn parse(window: &sdl3::video::Window, event_pump: &mut EventPump) -> Result<Chip8Configuration, &'static str> {
        // Reads rom path and other configuration from the command line
        let mut rom_path = String::from("");
        let mut clock_per_sec = 500;
        let mut background_color = 0xFF000000; // Black
        let mut foreground_color = 0xFFFFFFFF; // White
        let mut window_size = None;
        let mut is_fullscreen = false;
        let mut is_drawsync = true;
        let mut is_vsync = true;

        let mut args =  env::args().skip(1).peekable();
        loop {
            // Exits iterator at the end of the environment args
            let arg = match args.next() {
                Some(arg) => arg,
                None => break
            };

            // Parses command parameters and the numerical postfix
            let arg_type = arg.trim_end_matches(char::is_numeric);
            match arg_type {
                "-bg" | "-background" => {
                    // Parses background color
                    match parse_color(&mut args, arg.as_str(), arg_type) {
                        Ok(color) => background_color = color,
                        Err(ParseColorError::Missing) => return Err("Background color is missing!"),
                        Err(ParseColorError::MissingBlue) => return Err("Missing blue value for background!"),
                        Err(ParseColorError::Invalid) => return Err("Background color is not a number!"),
                        Err(ParseColorError::InvalidRgb) => return Err("Invalid rgb value for background!"),
                        Err(ParseColorError::InvalidRed) => return Err("Invalid red value for background!"),
                        Err(ParseColorError::InvalidGreen) => return Err("Invalid green value for background!"),
                        Err(ParseColorError::InvalidBlue) => return Err("Invalid blue value for background!")
                    };
                }

                "-c" | "-clock" => {
                    // Reads clock speed argument with or without a space
                    match parse_first_number(&mut args, arg.as_str(), arg_type) {
                        Ok(hz) => clock_per_sec = hz,
                        Err(ParseError::Missing) => return Err("Clock speed is missing!"),
                        Err(ParseError::Invalid) => return Err("Clock speed is not a number!")
                    }
                }

                "-fg" | "-foreground" => {
                    // Parses foreground color
                     match parse_color(&mut args, arg.as_str(), arg_type) {
                        Ok(color) => foreground_color = color,
                        Err(ParseColorError::Missing) => return Err("Foreground color is missing!"),
                        Err(ParseColorError::MissingBlue) => return Err("Missing blue value for foreground!"),
                        Err(ParseColorError::Invalid) => return Err("Foreground color is not a number!"),
                        Err(ParseColorError::InvalidRgb) => return Err("Invalid rgb value for foreground!"),
                        Err(ParseColorError::InvalidRed) => return Err("Invalid red value for foreground!"),
                        Err(ParseColorError::InvalidGreen) => return Err("Invalid green value for foreground!"),
                        Err(ParseColorError::InvalidBlue) => return Err("Invalid blue value for foreground!")
                    };
                }

                "-fs" | "fullscreen" => is_fullscreen = true,

                "-h" | "-help" => {
                    print!("\
                        chip8-emulator <Rom path> <Options>\n\
                        Options:\n    \
                        -bg -background   <RGB color> | <Red> <Green> <Blue>  (default: 0, 0, 0)\n    \
                        -c  -clock        <Cycles per second>                 (default: 500 hz)\n    \
                        -fg -foreground   <RGB color> | <Red> <Green> <Blue>  (default: 255, 255, 255)\n    \
                        -fs -fullscreen   Turns on fullscreen mode\n    \
                        -h  -help         Displays this help message\n        \
                            -nodrawsync   Turns off the 60hz draw sync\n        \
                            -novsync      Turns off vertical sync\n    \
                        -sf -scalefactor  <Scale factor of 64x32 screen>\n    \
                        -w  -windowsize   <Pixel width> <Pixel height>\
                    ");
                    return Err("")
                }

                "-nodrawsync" => is_drawsync = false,
                "-novsync" => is_vsync = false,
                
                "-sf" | "-scalefactor" => {
                    // Reads scale factor argument with or without a space
                    match parse_first_number(&mut args, arg.as_str(), arg_type) {
                        Ok(scale) => window_size = Some(Err(scale)),
                        Err(ParseError::Missing) => return Err("Scale factor is missing!"),
                        Err(ParseError::Invalid) => return Err("Scale factor is not a number!")
                    }
                }

                "-w" | "-windowsize" => {
                    // Reads window width argument with or without a space
                    let mut size = (0, 0);
                    size.0 = match parse_first_number(&mut args, arg.as_str(), arg_type) {
                        Ok(width) => width,
                        Err(ParseError::Missing) => return Err("Window width is missing!"),
                        Err(ParseError::Invalid) => return Err("Window width is not a number!")
                    };

                    // Parses the next argument as the window height
                    size.1 = match parse_next_number(&mut args) {
                        Ok(height) => height,
                        Err(ParseError::Missing) => return Err("Window height is missing!"),
                        Err(ParseError::Invalid) => return Err("Window height is not a number!")
                    };
                    window_size = Some(Ok(size));
                }

                // Accepts at most one rom path
                _ => match rom_path.as_str() {
                    "" => rom_path = arg,
                    _ => return Err("More than one rom paths found!")
                }
            }
        }

        // Tries through a gui if the command line fails to find a rom path
        if rom_path == "" {
            // Initializes channels because file dialogs are asynchronous
            let (sender, receiver) = mpsc::channel::<PathBuf>();

            let dialog_callback: DialogCallback = Box::new(move |paths, _| {
                // Reads the rom path from dialog if provided
                let rom = match paths {
                    Ok(paths) => paths[0].clone(),
                    Err(_) => PathBuf::from("")
                };

                // Sends the rom path to the main thread
                if sender.send(rom).is_err() {
                    println!("Failed to send rom path from dialog!")
                }
            });

            // Calls the asynchronous open file dialog
            if show_open_file_dialog(&[], None::<&str>, false, Some(window), dialog_callback).is_err() {
                return Err("Failed to open file dialog!")
            }

            // Receives the rom path from the open file dialog
            rom_path = loop {
                // Pumps events so the dialog can function
                event_pump.pump_events();
                match receiver.try_recv() {
                    Ok(path) => match path.into_os_string().into_string() {
                        Ok(path) => break path,
                        Err(_) => return Err("Failed to receive rom path from dialog!")
                    }
                    // sleeping prevents a spin lock
                    Err(_) => sleep(Duration::from_millis(1))
                };
            };

            // Terminates without a rom path
            if rom_path == "" {
                return Err("Missing path to the rom!")
            }
        }

        Ok(Chip8Configuration{rom_path, clock_hz: clock_per_sec, background_color, foreground_color, window_size, is_fullscreen, is_drawsync, is_vsync})
    }
}

enum ParseError {
    Missing,
    Invalid
}

fn parse_first_number<I: Iterator<Item = String>>(args: &mut I, arg: &str, arg_type: &str) -> Result<u32, ParseError> {
    // Reads number with or without a space
    let value = match arg.len() == arg_type.len() {
        false => String::from(&arg[arg_type.len()..]),
        true => match args.next() {
            Some(arg) => arg,
            None => return Err(ParseError::Missing)
        }
    };

    // Converts from a string to a number
    match value.parse::<u32>() {
        Ok(arg) => Ok(arg),
        Err(_) => return Err(ParseError::Invalid)
    }
}

fn parse_next_number<I: Iterator<Item = String>>(args: &mut Peekable<I>) -> Result<u32, ParseError> {
    match args.peek() {
        Some(value) => match value.parse::<u32>() {
            Ok(arg) => {
                args.next();
                Ok(arg)
            }
            Err(_) => Err(ParseError::Invalid)
        }
        None => Err(ParseError::Missing)
    }
}

enum ParseColorError {
    Missing,
    MissingBlue,
    Invalid,
    InvalidRgb,
    InvalidRed,
    InvalidGreen,
    InvalidBlue,
}

fn parse_color<I: Iterator<Item = String>>(args: &mut Peekable<I>, arg: &str, arg_type: &str) -> Result<u32, ParseColorError> {
    // Reads color argument with or without a space
    let red = match parse_first_number(args, arg, arg_type) {
        Ok(hue) => hue,
        Err(ParseError::Missing) => return Err(ParseColorError::Missing),
        Err(ParseError::Invalid) => return Err(ParseColorError::Invalid),
    };

    // Parses color arguments as the green and blue values
    let green = parse_next_number(args);
    let blue = parse_next_number(args);

    // Sets color from an rgb value or 3 r, g, and b values
    return match (green, blue) {
        (Err(_), Err(_)) => {
            // Terminates if the rgb value has an alpha value
            if red > 0xFFFFFF { return Err(ParseColorError::InvalidRgb) }
            Ok(0xFF000000 | red)
        }

        (Ok(_), Err(_)) => Err(ParseColorError::MissingBlue),
        (Err(_), Ok(_)) => Ok(0),

        (Ok(green), Ok(blue)) => {
            // Terminates if any color exceeds the byte limit
            if red > 0xFF { return Err(ParseColorError::InvalidRed) }
            if green > 0xFF { return Err(ParseColorError::InvalidGreen) }
            if blue > 0xFF { return Err(ParseColorError::InvalidBlue) }

            Ok(u32::from_ne_bytes([blue as u8, green as u8, red as u8, 0xFF]))
        }
    };
}