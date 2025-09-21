# Crystadusa's chip8 emulator in rust
### About
I wrote this demo to learn about the rust programming language and emulation. I chose [chip8](https://chip-8.github.io/links/) because it was simple to implement. This is only guaranteed to be compatible with the original chip8 specification.

### Command line syntax
chip8-emulator \<Rom path\> \<Options\>\
Options:
* -c -clock \<The number of cycles per second\> (default: 500 hz)

### Build
This project is simply built with "cargo build --release".\
Remember to set a corresponding cmake generator on windows with a developer shell like MinGW32 or WSL.

### Useful links
* compatibility of chip8 extensions: https://chip-8.github.io/extensions/#chip-48
* interpreter disassembly: https://web.archive.org/web/20190819144645/http://laurencescotford.co.uk/wp-content/uploads/2013/08/CHIP-8-Interpreter-Disassembly.pdf
* test roms: https://github.com/Timendus/chip8-test-suite?tab=readme-ov-file
* variant opcode table: https://chip8.gulrak.net/
* other resources: https://chip-8.github.io/links/
