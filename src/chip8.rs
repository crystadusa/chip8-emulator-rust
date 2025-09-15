// Indices into an array of chip8 keys
pub enum Key {
    X1, X2, X3, XC,
    X4, X5, X6, XD,
    X7, X8, X9, XE,
    XA, X0, XB, XF,
}

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
    pub keyboard: Box<[u8; 16]>,

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

        // Initializes registers and memory to zero, and program counter to 0x200
        Ok(Chip8 {ram, frame_buffer: Box::new([0; 64 * 32]), stack: Box::new([0; 16]), keyboard: Box::new([0; 16]),
            program_counter: 0x200, index_register: 0, stack_pointer: 0, delay_timer: 0, sound_timer: 0,
            general_registers: [0; 16]})
    }

    pub fn run(&mut self) {
        // Decrements timers at the start of frame
        if self.delay_timer > 0 { self.delay_timer -= 1; }
        if self.sound_timer > 0 { self.sound_timer -= 1; }

        // Does nothing if the delay timer is not 0
        if self.delay_timer > 0 { return }

        // Runs 480 instructions a second or 8 per frame at 60 fps
        let instructions = self.ram.as_chunks::<2>().0;
        for _ in 0..8 {
            // Parses opcode for its values
            // let first = u16::from_be_bytes(instruction[..2].try_into().expect("Error"));
            let opcode = instructions[usize::from(self.program_counter / 2)];
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
                    0x0EE => (),
                    // opcode SYS addr - jumps to machine code runtime (ignored by modern interpreters)
                    _ => ()
                }
                // opcode JP addr - jumps to address nnn
                0x1 => (),
                // opcode CALL Vx, byte - calls subroutine at nnn
                0x2 => (),
                // opcode SE Vx, byte - skips instruction if register x == kk
                0x3 => (),
                // opcode SNE Vx, byte - skips instruction if register x != kk
                0x4 => (),
                0x5 => match op3 {
                    // opcode SE Vx, Vy - skips instruction if register x == register y
                    0x0 => (),
                    _ => {
                        println!("Unsupported opcode!");
                        return;
                    }
                },
                // opcode LD Vx, byte - kk is loaded in register x
                0x6 => (),
                // opcode ADD Vx, byte - register x plus kk is loaded in register x
                0x7 => (),
                0x8 => match op3 {
                    // opcode LD Vx, Vy - registered y is loaded in register x
                    0x0 => (),
                    // opcode OR Vx, Vy - register x = register x | register y
                    0x1 => (),
                    // opcode AND Vx, Vy - register x = register x & register y
                    0x2 => (),
                    // opcode XOR Vx, Vy - register x = register x ^ register y
                    0x3 => (),
                    // overflows in the following ops are stored in the flags register
                    // opcode AND Vx, Vy - register x = register x + register y
                    0x4 => (),
                    // opcode SUB Vx, Vy - register x = register x - register y
                    0x5 => (),
                    // opcode SHR Vx, Vy - register x is shifted to the right by one
                    0x6 => (),
                    // opcode SUBN Vx, Vy - register x = register y - register x
                    0x7 => (),
                    // opcode SHL Vx, Vy - register x is shifted to the left by one
                    0xE => (),
                    _ => {
                        println!("Unsupported opcode!");
                        return;
                    }
                }
                0x9 => match op3 {
                    // opcode SNE Vx, Vy - skips instruction if register x != register y
                    0x0 => (),
                    _ => {
                        println!("Unsupported opcode!");
                        return;
                    }
                }
                // opcode LD I, addr - nnn is loaded in the index register
                0xA => (),
                // opcode JP V0, addr - jumps to address nnn + register 0
                0xB => (),
                // opcode RND Vx, byte - register x = random byte & register x
                0xC => (),
                // opcode DRW Vx, Vy, n - draws n byte sized sprite at the index register
                // the x position is in the x register and the y position is in the y register
                // the flag register is set when a old sprite xor'd onto the screen erases another
                // out of bounds coordinates wrap around the screen
                0xD => (),
                0xE => match opcode[1] {
                    // opcode SKP Vx - skips instruction if the key value in register x is pressed
                    0x9E => (),
                    // opcode SKP Vx - skips instruction if the key value in register x is not pressed
                    0xA1 => (),
                    _ => {
                        println!("Unsupported opcode!");
                        return;
                    }
                }
                0xF => match opcode[1] {
                    // opcode LD Vx, DT - the display timer is loaded in register x
                    0x07 => (),
                    // opcode LD Vx, K - waits for a key press, then the key is loaded in register x
                    0x0A => (),
                    // opcode LD DT, VX - register x is loaded in the display timer
                    0x15 => (),
                    // opcode LD ST, Vx - register x is loaded in the sound timer
                    0x18 => (),
                    // opcode ADD I, Vx - index register = index register + register x
                    0x1E => (),
                    // opcode LD B, Vx - address of the sprite for the digit in register x is loaded in register x
                    0x29 => (),
                    // opcode LD F, Vx - the BCD representation of register x is loaded at the index register
                    // index register = 5 times register x
                    0x33 => (),
                    // opcode LD [I], Vx - registers 0 to x are loaded at the index register
                    // index register = index register + x + 1
                    0x55 => (),
                    // opcode LD Vx, [I] - memory starting at the index register is loaded in registers 0 to x
                    // index register = index register + x + 1
                    0x65 => (),
                    _ => {
                        println!("Unsupported opcode!");
                        return;
                    }
                }
                _ => {
                    println!("Unsupported opcode!");
                    return;
                }
            }
            
            // Increments program counter by 2
            self.program_counter += 2;
            println!("{}", self.program_counter);
            if self.program_counter > 0xFFF {
                println!("Invalid program counter address!");
                return;
            }
        }
    }
}