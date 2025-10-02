// Namespace imports
use rand::{rngs::SmallRng, RngCore, SeedableRng};
use crate::{config::Chip8Configuration};

extern crate rand;

// Constants
const FLAGS_REGISTER: usize = 0xF;
pub const FRAME_BUFFER_WIDTH: u16 = 64;
pub const FRAME_BUFFER_HEIGHT: u16 = 32;
const FRAME_BUFFER_SIZE: usize = FRAME_BUFFER_WIDTH as usize * FRAME_BUFFER_HEIGHT as usize * 4;
const MAX_RAM_ADDRESS: u16 = 0x1000 - 0x160; // Last 0x160 bytes are reserved
const SPRITE_WIDTH: u8 = 8;

// Pixel data for numerical font in the chip8 interpreter
const FONTS: [u8; 0x50] = [
    0xF0, 0x90, 0x90, 0x90, 0xF0, // 0x0
    0x20, 0x60, 0x20, 0x20, 0x70, // 0x1
    0xF0, 0x10, 0xF0, 0x80, 0xF0, // 0x2
    0xF0, 0x10, 0xF0, 0x10, 0xF0, // 0x3
    0x90, 0x90, 0xF0, 0x10, 0x10, // 0x4
    0xF0, 0x80, 0xF0, 0x10, 0xF0, // 0x5
    0xF0, 0x80, 0xF0, 0x90, 0xF0, // 0x6
    0xF0, 0x10, 0x20, 0x40, 0x40, // 0x7
    0xF0, 0x90, 0xF0, 0x90, 0xF0, // 0x8
    0xF0, 0x90, 0xF0, 0x10, 0xF0, // 0x9
    0xF0, 0x90, 0xF0, 0x90, 0x90, // 0xA
    0xE0, 0x90, 0xE0, 0x90, 0xE0, // 0xB
    0xF0, 0x80, 0x80, 0x80, 0xF0, // 0xC
    0xE0, 0x90, 0x90, 0x90, 0xE0, // 0xD
    0xF0, 0x80, 0xF0, 0x80, 0xF0, // 0xE
    0xF0, 0x80, 0xF0, 0x80, 0x80, // 0xF
];

// The chip8 state which can be initialized and ran
pub struct Chip8 {
    ram: Box<[u8; MAX_RAM_ADDRESS as usize]>,
    pub frame_buffer: Box<[u8; FRAME_BUFFER_SIZE]>,
    stack: Box<[u16; 12]>,
    pub key_released: Box<[bool; 16]>,
    pub keyboard: Box<[bool; 16]>,

    random_generator: SmallRng,
    pub remaining_samples: Option<i32>,
    clock_hz: u32,
    clock_buffer: u32,
    pub background_color: u32,
    foreground_color: u32,
    is_drawsync: bool,

    program_counter: u16,
    index_register: u16,
    stack_pointer: u8,
    delay_timer: u8,
    pub sound_timer: u8,
    general_registers: [u8; 16]
}

impl Chip8 {
    pub fn init(config: &Chip8Configuration) -> Result<Chip8, &'static str> {
        // Reads rom from file
        let rom = match std::fs::read(&config.rom_path) {
            Ok(file) => file,
            Err(_) => return Err("Path to the rom is invalid!")
        };

        // Copies font data and rom into ram
        if rom.len() > MAX_RAM_ADDRESS as usize - 0x200 {
            return Err("The rom is too large for the ram!")
        }

        let mut ram = Box::new([0; MAX_RAM_ADDRESS as usize]);
        ram[..FONTS.len()].clone_from_slice(&FONTS);
        ram[0x200..0x200 + rom.len()].clone_from_slice(&rom);

        // Initializes non cryptographic random number generator
        let rng = SmallRng::from_os_rng();

        // Initializes registers and memory to zero, and program counter to 0x200
        Ok(Chip8 {ram, frame_buffer: Box::new([0; FRAME_BUFFER_SIZE]), stack: Box::new([0; 12]), key_released: Box::new([false; 16]), keyboard: Box::new([false; 16]),
            random_generator: rng, remaining_samples: None, background_color: config.background_color, foreground_color: config.foreground_color, is_drawsync: config.is_drawsync,
            clock_hz: config.clock_hz, clock_buffer: 0, program_counter: 0x200, index_register: 0, stack_pointer: 0, delay_timer: 0, sound_timer: 0, general_registers: [0; 16]})
    }

    pub fn run(&mut self) -> Option<&'static str> {
        // Decrements timers at the start of frame
        if self.delay_timer > 0 { self.delay_timer -= 1; }
        if self.sound_timer > 0 { self.sound_timer -= 1; }

        // Subtracts 1/60th of a second increments from 1/clock_hz second increments to calculate cycles in a frame
        // A buffer transfers the time not emulated to the next frame
        self.clock_buffer += self.clock_hz;
        let cycles = self.clock_buffer / 60;
        self.clock_buffer %= 60;

        // Runs self.clock_hz instructions a second at 60 fps
        'run_loop: for cycle in 0..cycles {
            // Terminates if the program counter is out of range or unaligned
            if self.program_counter < 0x200 || self.program_counter >= MAX_RAM_ADDRESS || self.program_counter % 2 == 1{
                return Some("Invalid program counter address!")
            }

            // Parses opcode for its values
            let opcode = &self.ram[self.program_counter as usize..self.program_counter as usize + 2];
            let (op0, op1, op2, op3) = (opcode[0] >> 4, opcode[0] & 0xF, opcode[1] >> 4, opcode[1] & 0xF);
            let (x, y, n) = (op1, op2, op3);
            let kk = opcode[1];
            let nnn = ((op1 as u16) << 8) | opcode[1] as u16;

            // Parses rom instructions
            match op0 {
                0x0 => match nnn {
                    // opcode CLS - clears the display
                    0x0E0 => for pixel in self.frame_buffer.chunks_exact_mut(4) {
                        pixel.copy_from_slice(&self.background_color.to_le_bytes());
                    }

                    // opcode RET - returns from subroutine
                    0x0EE => {
                        if self.stack_pointer as usize == 0 {
                            return Some("Stack underflow on function return!")
                        }
                        self.stack_pointer -= 1;
                        self.program_counter = self.stack[self.stack_pointer as usize];
                    },
                    
                    // opcode SYS addr - jumps to machine code runtime (ignored by modern interpreters)
                    _ => ()
                }

                // opcode JP addr - jumps to address nnn
                0x1 => self.program_counter = nnn - 2,

                // opcode CALL Vx, byte - calls subroutine at nnn
                0x2 => {
                    if self.stack_pointer as usize == self.stack.len() {
                        return Some("Stack overflow on function call!")
                    }
                    self.stack[self.stack_pointer as usize] = self.program_counter;
                    self.stack_pointer += 1;
                    self.program_counter = nnn - 2;
                },

                // opcode SE Vx, byte - skips instruction if register x == kk
                0x3 => if self.general_registers[x as usize] == kk { self.program_counter += 2; },

                // opcode SNE Vx, byte - skips instruction if register x != kk
                0x4 => if self.general_registers[x as usize] != kk { self.program_counter += 2; },

                0x5 => match op3 {
                    // opcode SE Vx, Vy - skips instruction if register x == register y
                    0x0 => if self.general_registers[x as usize] == self.general_registers[y as usize] { self.program_counter += 2; },
                    _ => return Some("Unsupported opcode!")
                },

                // opcode LD Vx, byte - kk is loaded in register x
                0x6 => self.general_registers[x as usize] = kk,

                // opcode ADD Vx, byte - register x plus kk is loaded in register x
                0x7 => self.general_registers[x as usize] = self.general_registers[x as usize].wrapping_add(kk),

                0x8 => match op3 {
                    // opcode LD Vx, Vy - registered y is loaded in register x
                    0x0 => self.general_registers[x as usize] = self.general_registers[y as usize],

                    // the following opcodes reset the flags register to 0 
                    // opcode OR Vx, Vy - register x = register x | register y
                    0x1 => {
                        self.general_registers[x as usize] |= self.general_registers[y as usize];
                        self.general_registers[FLAGS_REGISTER] = 0;
                    }

                    // opcode AND Vx, Vy - register x = register x & register y
                    0x2 => {
                        self.general_registers[x as usize] &= self.general_registers[y as usize];
                        self.general_registers[FLAGS_REGISTER] = 0;
                    }

                    // opcode XOR Vx, Vy - register x = register x ^ register y
                    0x3 => {
                        self.general_registers[x as usize] ^= self.general_registers[y as usize];
                        self.general_registers[FLAGS_REGISTER] = 0;
                    }

                    // opcode AND Vx, Vy - register x = register x + register y
                    // sets the flags register to 1 on overflow
                    0x4 => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_add(self.general_registers[y as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
                    },

                    // opcode SUB Vx, Vy - register x = register x - register y
                    // sets the flags register to 0 on overflow
                    0x5 => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_sub(self.general_registers[y as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = !overflow as u8;
                    },

                    // opcode SHR Vx, Vy - register x is shifted to the right by one
                    // sets the flags register to 1 when shifting out a 1 bit
                    0x6 => {
                        let value = self.general_registers[y as usize];
                        self.general_registers[x as usize] = value >> 1;
                        self.general_registers[FLAGS_REGISTER] = value & 1;
                    },

                    // opcode SUBN Vx, Vy - register x = register y - register x
                    // sets the flags register to 0 on overflow
                    0x7 => {
                        let (result, overflow) = self.general_registers[y as usize].overflowing_sub(self.general_registers[x as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = !overflow as u8;
                    },

                    // opcode SHL Vx, Vy - register x is shifted to the left by one
                    // sets the flags register to 1 when shifting out a 1 bit
                    0xE => {
                        let value = self.general_registers[y as usize];
                        self.general_registers[x as usize] = value << 1;
                        self.general_registers[FLAGS_REGISTER] = value >> 7;
                    },
                    _ => return Some("Unsupported opcode!")
                }

                0x9 => match op3 {
                    // opcode SNE Vx, Vy - skips instruction if register x != register y
                    0x0 => if self.general_registers[x as usize] != self.general_registers[y as usize] { self.program_counter += 2; },
                    _ => return Some("Unsupported opcode!")
                }

                // opcode LD I, addr - nnn is loaded in the index register
                0xA => self.index_register = nnn,

                // opcode JP V0, addr - jumps to address nnn + register 0
                0xB => self.program_counter = nnn + self.general_registers[0] as u16 - 2,

                // opcode RND Vx, byte - register x = random byte & register x
                0xC => self.general_registers[x as usize] = self.random_generator.next_u64() as u8 & kk,

                // opcode DRW Vx, Vy, n - draws n byte sized sprite at the index register
                // the x position is in the x register and the y position is in the y register
                // the flag register is set when a old sprite xor'd onto the screen erases another
                // out of bounds starting coordinates wrap around the screen
                // sprites partially drawn offscreen are clipped
                // waits for the next vsync on completion
                0xD => {
                    // A draw doesn't erase a sprite until proven otherwise
                    self.general_registers[FLAGS_REGISTER] = 0;

                    // Wraps around the screen if the sprite is drawing offscreen
                    let x = self.general_registers[x as usize] % FRAME_BUFFER_WIDTH as u8;
                    let y = self.general_registers[y as usize] % FRAME_BUFFER_HEIGHT as u8;

                    // Terminates if the draw is accessing invalid ram
                    if self.index_register + n as u16 - 1 >= MAX_RAM_ADDRESS {
                        return Some("Invalid memory access in draw!")
                    }

                    // Iterates the n rows of the sprite
                    for i in 0..n {
                        // Caps y at the screen height for vertical screen clipping
                        if y + i >= FRAME_BUFFER_HEIGHT as u8 { break }

                        // Iterates the 8 columns of the sprite
                        let row_data = self.ram[self.index_register as usize + i as usize];
                        let row_index = (y + i) as u16 * FRAME_BUFFER_WIDTH;
                        for j in 0..SPRITE_WIDTH {
                            // Caps x at the screen width for horizontal screen clipping
                            if x + j >= FRAME_BUFFER_WIDTH as u8 { break }

                            // The row data is a bit field for the pixel data
                            let is_pixel_set = (row_data >> (SPRITE_WIDTH - 1 - j)) & 1;

                            // Xor's the sprite with the frame buffer to draw
                            // Sets the flags register to 1 if another sprite is erased
                            let pixel_index = row_index + (x + j) as u16;
                            let pixel = &mut self.frame_buffer[pixel_index as usize * 4..(pixel_index as usize + 1) * 4];
                            match (pixel == self.foreground_color.to_le_bytes(), is_pixel_set) {
                                (false, 0) => pixel.copy_from_slice(&self.background_color.to_le_bytes()),
                                (false, _) | (true, 0) => pixel.copy_from_slice(&self.foreground_color.to_le_bytes()),
                                (true, _) => {
                                    self.general_registers[FLAGS_REGISTER] = 1;
                                    pixel.copy_from_slice(&self.background_color.to_le_bytes());
                                }
                            }
                        }
                    }

                    // Waits until next vertical blank
                    if self.is_drawsync {
                        self.program_counter += 2;
                        break 'run_loop
                    }
                },

                0xE => match opcode[1] {
                    // opcode SKP Vx - skips instruction if the key value in register x is pressed
                    0x9E => if self.keyboard[self.general_registers[x as usize] as usize & 0xF] { self.program_counter += 2; },
                    
                    // opcode SKP Vx - skips instruction if the key value in register x is not pressed
                    0xA1 => if !self.keyboard[self.general_registers[x as usize] as usize & 0xF] { self.program_counter += 2; },
                    _ => return Some("Unsupported opcode!")
                }

                0xF => match opcode[1] {
                    // opcode LD Vx, DT - the delay timer is loaded in register x
                    0x07 => self.general_registers[x as usize] = self.delay_timer,
                    
                    // opcode LD Vx, K - waits for a key press, then the key is loaded in register x
                    0x0A => {
                        let mut is_key_pressed = false;
                        for i in 0..self.keyboard.len() {
                            // Iterates to find a released key
                            if self.key_released[i] {
                                // Handles the release to avoid repeat detections
                                self.key_released[i] = false;

                                // Returns the released key in register x
                                self.general_registers[x as usize] = i as u8;
                                is_key_pressed = true;
                                break
                            }
                        }

                        // Waits if no key is pressed
                        if !is_key_pressed { break 'run_loop }
                    }

                    // opcode LD DT, VX - register x is loaded in the delay timer
                    0x15 => self.delay_timer = self.general_registers[x as usize],

                    // opcode LD ST, Vx - register x is loaded in the sound timer
                    // a value of 1 is not responded to on original hardware
                    0x18 => {
                        self.sound_timer = self.general_registers[x as usize];

                        if self.sound_timer > 1 {
                            // Calculates the number of audio samples in the sound timer's duration
                            let cycles_before_timer = cycles * 60 + self.clock_buffer - self.clock_hz;
                            let elapsed_frame_samples = ((cycle + 1) * 60 - cycles_before_timer) as f32 * (48000.0 / 60.0 / self.clock_hz as f32);
                            self.remaining_samples = Some(self.sound_timer as i32 * (48000 / 60) - elapsed_frame_samples as i32);
                        }
                    },

                    // opcode ADD I, Vx - index register = index register + register x
                    0x1E => self.index_register += self.general_registers[x as usize] as u16,

                    // opcode LD B, Vx - address of the sprite for the digit in register x is loaded in the index register
                    // index register = register x * 5
                    0x29 => self.index_register = self.general_registers[x as usize & 0xF] as u16 * 5,

                    // opcode LD F, Vx - the BCD representation of register x is loaded at the index register
                    0x33 => {
                        // Terminates if the BCD store is accessing invalid ram
                        if self.index_register < 0x200 || self.index_register + 2 >= MAX_RAM_ADDRESS {
                            return Some("Invalid memory access in BCD store!")
                        }

                        self.ram[self.index_register as usize]     = self.general_registers[x as usize] / 100;
                        self.ram[self.index_register as usize + 1] = self.general_registers[x as usize] / 10 % 10;
                        self.ram[self.index_register as usize + 2] = self.general_registers[x as usize] % 10;
                    },

                    // opcode LD [I], Vx - registers 0 to x are loaded at the index register
                    // index register = index register + x + 1
                    0x55 => {
                        // Terminates if the store is accessing invalid ram
                        let max_ram_access = self.index_register + x as u16;
                        if self.index_register < 0x200 || max_ram_access >= MAX_RAM_ADDRESS {
                            return Some("Invalid memory access in store!")
                        }

                        let destination = &mut self.ram[self.index_register as usize..max_ram_access as usize + 1];
                        destination.copy_from_slice(&self.general_registers[0..x as usize + 1]);
                        self.index_register += x as u16 + 1;
                    },

                    // opcode LD Vx, [I] - memory starting at the index register is loaded in registers 0 to x
                    // index register = index register + x + 1
                    0x65 => {
                        // Terminates if the load is accessing invalid ram
                        let max_ram_access = self.index_register + x as u16;
                        if max_ram_access >= MAX_RAM_ADDRESS {
                            return Some("Invalid memory access in load!");
                        }

                        let source = &self.ram[self.index_register as usize..max_ram_access as usize + 1];
                        self.general_registers[0..x as usize + 1].copy_from_slice(source);
                        self.index_register += x as u16 + 1;
                    },
                    _ => return Some("Unsupported opcode!")
                }
                _ => return Some("Unsupported opcode!")
            }
            
            // Increments program counter by 2
            self.program_counter += 2;
        }

        // Keeps track of the previous keyboard state to know when a key is pressed or released
        self.key_released.fill(false);
        None
    }
}