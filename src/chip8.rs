// Namespace imports
use rand::{rngs::SmallRng, RngCore, SeedableRng};

extern crate rand;

// Constants
const FLAGS_REGISTER: usize = 0xF;
const FRAME_BUFFER_WIDTH: u16 = 64;
const FRAME_BUFFER_HEIGHT: u16 = 32;
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
    ram: Box<[u8; 0x1000]>,
    pub frame_buffer: Box<[u8; 64 * 32]>,
    stack: Box<[u16; 16]>,
    previous_keyboard: Box<[u8; 16]>,
    pub keyboard: Box<[u8; 16]>,
    random_generator: SmallRng,

    program_counter: u16,
    index_register: u16,
    stack_pointer: u8,
    delay_timer: u8,
    pub sound_timer: u8,
    general_registers: [u8; 16]
}

impl Chip8 {
    pub fn init() -> Result<Chip8, &'static str> {
        // Reads file path from command line
        let rom_path = match std::env::args().skip(1).next() {
            Some(path) => path,
            None => return Err("Missing path to the rom!")
        };

        // Reads rom from file
        let rom = match std::fs::read(rom_path) {
            Ok(file) => file,
            Err(_) => return Err("Path to the rom is invalid!")
        };

        // Copies font data and rom into ram
        if rom.len() > 0xFFF - 0x200 {
            return Err("The rom is too large for the ram!")
        }

        let mut ram = Box::new([0; 4096]);
        ram[..FONTS.len()].clone_from_slice(&FONTS);
        ram[0x200..0x200 + rom.len()].clone_from_slice(&rom);

        // Initializes non cryptographic random number generator
        let rng = rand::rngs::SmallRng::from_os_rng();

        // Initializes registers and memory to zero, and program counter to 0x200
        Ok(Chip8 {ram, frame_buffer: Box::new([0; 64 * 32]), stack: Box::new([0; 16]), previous_keyboard: Box::new([0; 16]), keyboard: Box::new([0; 16]),
            random_generator: rng, program_counter: 0x200, index_register: 0, stack_pointer: 0, delay_timer: 0, sound_timer: 0,
            general_registers: [0; 16]})
    }

    pub fn run(&mut self) -> Option<&'static str> {
        // Decrements timers at the start of frame
        if self.delay_timer > 0 { self.delay_timer -= 1; }
        if self.sound_timer > 0 { self.sound_timer -= 1; }

        // Runs 480 instructions a second or 8 per frame at 60 fps
        'run_loop: for _ in 0..8 {
            // Terminates if the program counter is out of range or unaligned
            if self.program_counter < 0x200 || self.program_counter > 0xFFF || self.program_counter % 2 == 1{
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
                    0x0E0 => self.frame_buffer.fill(0),

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

                    // opcode OR Vx, Vy - register x = register x | register y
                    0x1 => self.general_registers[x as usize] |= self.general_registers[y as usize],

                    // opcode AND Vx, Vy - register x = register x & register y
                    0x2 => self.general_registers[x as usize] &= self.general_registers[y as usize],

                    // opcode XOR Vx, Vy - register x = register x ^ register y
                    0x3 => self.general_registers[x as usize] ^= self.general_registers[y as usize],

                    // overflows in the following ops are stored in the flags register
                    // opcode AND Vx, Vy - register x = register x + register y
                    0x4 => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_add(self.general_registers[y as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
                    },

                    // opcode SUB Vx, Vy - register x = register x - register y
                    0x5 => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_sub(self.general_registers[y as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
                    },

                    // opcode SHR Vx, Vy - register x is shifted to the right by one
                    0x6 => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_shr(1);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
                    },

                    // opcode SUBN Vx, Vy - register x = register y - register x
                    0x7 => {
                        let (result, overflow) = self.general_registers[y as usize].overflowing_sub(self.general_registers[x as usize]);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
                    },

                    // opcode SHL Vx, Vy - register x is shifted to the left by one
                    0xE => {
                        let (result, overflow) = self.general_registers[x as usize].overflowing_shr(1);
                        self.general_registers[x as usize] = result;
                        self.general_registers[FLAGS_REGISTER] = overflow as u8;
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
                // out of bounds coordinates wrap around the screen
                0xD => 'draw: {
                    // A draw doesn't erase a sprite until proven otherwise
                    self.general_registers[FLAGS_REGISTER] = 0;

                    // Doesn't draw if the sprite is drawing offscreen
                    let x = self.general_registers[x as usize];
                    let y = self.general_registers[y as usize];
                    if x >= FRAME_BUFFER_WIDTH as u8 || y >= FRAME_BUFFER_HEIGHT as u8 { break 'draw }

                    // Terminates if the draw is accessing invalid ram
                    if self.index_register + n as u16 - 1 > 0xFFF {
                        return Some("Invalid memory access in draw!")
                    }

                    // Iterates the n rows of the sprite
                    for i in 0..n {
                        // Starts a row at a modulus y index for vertical screen wrapping
                        let row_index = (y + i) as u16 % FRAME_BUFFER_HEIGHT * FRAME_BUFFER_WIDTH;

                        // Iterates the 8 columns of the sprite
                        let row_data = self.ram[self.index_register as usize + i as usize];
                        for j in 0..SPRITE_WIDTH {
                            // Adds a modulus x to the index for horizontal screen wrapping
                            let pixel_index = row_index + (x + j) as u16 % FRAME_BUFFER_WIDTH;

                            // The row data is a bit field for the pixel data
                            let is_pixel_set = (row_data >> (SPRITE_WIDTH - 1 - j)) & 1;

                            // Sets the flags register to 1 if another sprite is erased
                            if self.frame_buffer[pixel_index as usize] == 1 && is_pixel_set == 1 {
                                self.general_registers[FLAGS_REGISTER] = 1;
                            }

                            // Xor's the sprite with the frame buffer to draw
                            self.frame_buffer[pixel_index as usize] ^= is_pixel_set;
                        }
                    }
                },

                0xE => match opcode[1] {
                    // opcode SKP Vx - skips instruction if the key value in register x is pressed
                    0x9E => if self.keyboard[self.general_registers[x as usize] as usize] == 1 { self.program_counter += 2; },
                    
                    // opcode SKP Vx - skips instruction if the key value in register x is not pressed
                    0xA1 => if self.keyboard[self.general_registers[x as usize] as usize] == 0 { self.program_counter += 2; },
                    _ => return Some("Unsupported opcode!")
                }

                0xF => match opcode[1] {
                    // opcode LD Vx, DT - the delay timer is loaded in register x
                    0x07 => self.general_registers[x as usize] = self.delay_timer,
                    
                    // opcode LD Vx, K - waits for a key press, then the key is loaded in register x
                    0x0A => {
                        let mut is_key_pressed = false;
                        for i in 0..self.keyboard.len() {
                            // Iterates to find a pressed key
                            if self.previous_keyboard[i] == 0 && self.keyboard[i] == 1 {
                                // Updates the key state from pressed to held
                                self.previous_keyboard[i] = 1;

                                // Returns the pressed key in register x
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
                    0x18 => self.sound_timer = self.general_registers[x as usize],

                    // opcode ADD I, Vx - index register = index register + register x
                    0x1E => self.index_register += self.general_registers[x as usize] as u16,

                    // opcode LD B, Vx - address of the sprite for the digit in register x is loaded in the index register
                    // index register = register x * 5
                    0x29 => self.index_register = self.general_registers[x as usize] as u16 * 5,

                    // opcode LD F, Vx - the BCD representation of register x is loaded at the index register
                    0x33 => {
                        // Terminates if the BCD store is accessing invalid ram
                        if self.index_register < 0x200 || self.index_register + 2 > 0xFFF {
                            return Some("Invalid memory access!")
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
                        if self.index_register < 0x200 || max_ram_access > 0xFFF {
                            return Some("Invalid memory access!")
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
                        if self.index_register < 0x200 || max_ram_access > 0xFFF {
                            return Some("Invalid memory access!");
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
        self.previous_keyboard.copy_from_slice(self.keyboard.as_slice());
        None
    }
}