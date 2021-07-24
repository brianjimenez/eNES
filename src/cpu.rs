use std::collections::HashMap;
use crate::opcodes;
use crate::bus::Bus;

const STACK: u16 = 0x0100;
const STACK_RESET: u8 = 0xFD;

#[non_exhaustive]
struct CpuFlags;

impl CpuFlags {
    // +-+-+-+-+-+-+-+-+
    // |N|V| |B|D|I|Z|C|
    // +-+-+-+-+-+-+-+-+
    //  7 6 5 4 3 2 1 0
    // C - Carry Flag
    // Z - Zero Flag
    // I - Interrupt Disable
    // D - Decimal Mode
    // B - Break Command
    // B2 - Break 2 Command
    // V - Overflow Flag
    // N - Negative Flag
    pub const CARRY: u8     = 0b0000_0001;
    pub const ZERO: u8      = 0b0000_0010;
    pub const INTERRUPT: u8 = 0b0000_0100;
    pub const DECIMAL: u8   = 0b0000_1000;
    pub const BREAK: u8     = 0b0001_0000;
    pub const BREAK2: u8    = 0b0010_0000;
    pub const OVERFLOW: u8  = 0b0100_0000;
    pub const NEGATIVE: u8  = 0b1000_0000;
}

#[derive(Debug)]
#[allow(non_camel_case_types)]
pub enum AddressingMode {
   Immediate,
   ZeroPage,
   ZeroPage_X,
   ZeroPage_Y,
   Absolute,
   Absolute_X,
   Absolute_Y,
   Indirect_X,
   Indirect_Y,
   NoneAddressing,
}

pub trait Mem {
    fn mem_read(&self, addr: u16) -> u8;

    fn mem_write(&mut self, addr: u16, data: u8);

    fn mem_read_u16(&self, pos: u16) -> u16 {
        let lo = self.mem_read(pos) as u16;
        let hi = self.mem_read(pos + 1) as u16;
        (hi << 8) | (lo as u16)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        let hi = (data >> 8) as u8;
        let lo = (data & 0xff) as u8;
        self.mem_write(pos, lo);
        self.mem_write(pos + 1, hi);
    }
}

pub struct CPU {
    pub register_a: u8,
    pub register_x: u8,
    pub register_y: u8,
    pub status: u8,
    pub program_counter: u16,
    pub stack_pointer: u8,
    pub bus: Bus,
}

impl Mem for CPU {
    fn mem_read(&self, addr: u16) -> u8 {
        self.bus.mem_read(addr)
    }

    fn mem_write(&mut self, addr: u16, data: u8) {
        self.bus.mem_write(addr, data)
    }
    fn mem_read_u16(&self, pos: u16) -> u16 {
        self.bus.mem_read_u16(pos)
    }

    fn mem_write_u16(&mut self, pos: u16, data: u16) {
        self.bus.mem_write_u16(pos, data)
    }
}

impl CPU {
    pub fn new(bus: Bus) -> Self {
        CPU {
            register_a: 0,
            register_x: 0,
            register_y: 0,
            stack_pointer: STACK_RESET,
            program_counter: 0,
            status: 0,
            bus: bus,
        }
    }

    fn get_operand_address(&self, mode: &AddressingMode) -> u16 {
       match mode {
           AddressingMode::Immediate => self.program_counter,

           AddressingMode::ZeroPage  => self.mem_read(self.program_counter) as u16,

           AddressingMode::Absolute => self.mem_read_u16(self.program_counter),

           AddressingMode::ZeroPage_X => {
               let pos = self.mem_read(self.program_counter);
               let addr = pos.wrapping_add(self.register_x) as u16;
               addr
           }
           AddressingMode::ZeroPage_Y => {
               let pos = self.mem_read(self.program_counter);
               let addr = pos.wrapping_add(self.register_y) as u16;
               addr
           }

           AddressingMode::Absolute_X => {
               let base = self.mem_read_u16(self.program_counter);
               let addr = base.wrapping_add(self.register_x as u16);
               addr
           }
           AddressingMode::Absolute_Y => {
               let base = self.mem_read_u16(self.program_counter);
               let addr = base.wrapping_add(self.register_y as u16);
               addr
           }

           AddressingMode::Indirect_X => {
               let base = self.mem_read(self.program_counter);

               let ptr: u8 = (base as u8).wrapping_add(self.register_x);
               let lo = self.mem_read(ptr as u16);
               let hi = self.mem_read(ptr.wrapping_add(1) as u16);
               (hi as u16) << 8 | (lo as u16)
           }

           AddressingMode::Indirect_Y => {
               let base = self.mem_read(self.program_counter);

               let lo = self.mem_read(base as u16);
               let hi = self.mem_read((base as u8).wrapping_add(1) as u16);
               let deref_base = (hi as u16) << 8 | (lo as u16);
               let deref = deref_base.wrapping_add(self.register_y as u16);
               deref
           }

           AddressingMode::NoneAddressing => {
               panic!("Addressing mode {:?} is not supported", mode);
           }
       }
   }

   /* Stack logic */
   fn stack_pop(&mut self) -> u8 {
       self.stack_pointer = self.stack_pointer.wrapping_add(1);
       self.mem_read((STACK as u16) + self.stack_pointer as u16)
   }

   fn stack_push(&mut self, data: u8) {
       self.mem_write((STACK as u16) + self.stack_pointer as u16, data);
       self.stack_pointer = self.stack_pointer.wrapping_sub(1)
   }

   fn stack_push_u16(&mut self, data: u16) {
       let hi = (data >> 8) as u8;
       let lo = (data & 0xff) as u8;
       self.stack_push(hi);
       self.stack_push(lo);
   }

   fn stack_pop_u16(&mut self) -> u16 {
       let lo = self.stack_pop() as u16;
       let hi = self.stack_pop() as u16;

       hi << 8 | lo
   }

    pub fn reset(&mut self) {
        self.register_a = 0;
        self.register_x = 0;
        self.register_y = 0;
        self.status = 0;
        //self.memory = [0; 0xFFFF];
        self.stack_pointer = STACK_RESET;
        self.program_counter = self.mem_read_u16(0xFFFC);
    }

    pub fn load(&mut self, program: Vec<u8>) {
        for i in 0..(program.len() as u16) {
            self.mem_write(0x0600 + i, program[i as usize]);
        }
        self.mem_write_u16(0xFFFC, 0x0600);
    }

    pub fn load_and_run(&mut self, program: Vec<u8>) {
        self.load(program);
        self.reset();
        self.run()
    }

    pub fn run(&mut self) {
        self.run_with_callback(|_| {});
    }

    pub fn run_with_callback<F>(&mut self, mut callback: F)
    where
        F: FnMut(&mut CPU),
    {
        let ref opcodes: HashMap<u8, &'static opcodes::OpCode> = *opcodes::OPCODES_MAP;

        loop {
            let code = self.mem_read(self.program_counter);
            println!("> PC: {:#04x}  |  Opcode: {:#04x}  |  SP: {:#04x}  |  A: {:#04x}  |  X: {:#04x}  |  Y: {:#04x}",
                self.program_counter, code, self.stack_pointer, self.register_a, self.register_x, self.register_y);
            self.program_counter += 1;
            let program_counter_state = self.program_counter;

            let opcode = opcodes.get(&code).expect(&format!("OpCode {:x} is not recognized", code));

            match code {
                /* LDA */
                0xA9 | 0xA5 | 0xB5 | 0xAD | 0xBD | 0xB9 | 0xA1 | 0xB1 => {
                    self.lda(&opcode.mode);
                }

                /* LDX */
                0xA2 | 0xA6 | 0xB6 | 0xAE | 0xBE => {
                    self.ldx(&opcode.mode);
                }

                /* LDY */
                0xA0 | 0xA4 | 0xB4 | 0xAC | 0xBC => {
                    self.ldy(&opcode.mode);
                }

                /* STA */
                0x85 | 0x95 | 0x8D | 0x9D | 0x99 | 0x81 | 0x91 => {
                    self.sta(&opcode.mode);
                }

                /* STX */
                0x86 | 0x96 | 0x8E => {
                    let addr = self.get_operand_address(&opcode.mode);
                    self.mem_write(addr, self.register_x);
                }

                /* CPX */
                0xE0 | 0xE4 | 0xEC => self.compare(&opcode.mode, self.register_x),

                /* JSR */
                0x20 => {
                    self.stack_push_u16(self.program_counter + 2 - 1);
                    let target_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = target_address;
                }
                /* RTS */
                0x60 => {
                    self.program_counter = self.stack_pop_u16() + 1;
                }

                /* ADC */
                0x69 | 0x65 | 0x75 | 0x6D | 0x7D | 0x79 | 0x61 | 0x71 => {
                    self.adc(&opcode.mode);
                }

                /* SBC */
                0xE9 | 0xE5 | 0xF5 | 0xED | 0xFD | 0xF9 | 0xE1 | 0xF1 => {
                    self.sbc(&opcode.mode);
                }

                /* AND */
                0x29 | 0x25 | 0x35 | 0x2d | 0x3d | 0x39 | 0x21 | 0x31 => {
                    self.and(&opcode.mode);
                }

                /* BNE */
                0xD0 => {
                    self.branch(self.status & CpuFlags::ZERO == 0);
                }

                /* BEQ */
                0xf0 => {
                    self.branch(self.status & CpuFlags::ZERO != 0);
                }

                /* BVS */
                0x70 => {
                    self.branch(self.status & CpuFlags::OVERFLOW != 0);
                }

                /* BVC */
                0x50 => {
                    self.branch(self.status & CpuFlags::OVERFLOW == 0);
                }

                /* BPL */
                0x10 => {
                    self.branch(self.status & CpuFlags::NEGATIVE == 0);
                }

                /* BMI */
                0x30 => {
                    self.branch(self.status & CpuFlags::NEGATIVE != 0);
                }

                /* BCS */
                0xb0 => {
                    self.branch(self.status & CpuFlags::CARRY != 0);
                }

                /* BCC */
                0x90 => {
                    self.branch(self.status & CpuFlags::CARRY == 0);
                }

                0xCA => self.dex(),
                0xAA => self.tax(),
                0x8A => self.txa(),
                0xE8 => self.inx(),
                0x00 => {
                    self.brk();
                    return
                }


                /* DEC */
                0xC6 | 0xD6 | 0xCE | 0xDE => {
                    self.dec(&opcode.mode);
                }

                /* Flags */
                0xd8 => {
                    self.status &= !CpuFlags::DECIMAL;
                }
                0x58 => {
                    self.status &= !CpuFlags::INTERRUPT;
                }
                0xb8 => {
                    self.status &= !CpuFlags::OVERFLOW;
                }
                0x18 => {
                    self.status &= !CpuFlags::CARRY;
                }
                0x38 => {
                    self.status |= CpuFlags::CARRY;
                }
                0x78 => {
                    self.status |= CpuFlags::INTERRUPT;
                }
                0xf8 => {
                    self.status |= CpuFlags::DECIMAL;
                }

                /* BIT */
                0x24 | 0x2c => {
                    self.bit(&opcode.mode);
                }

                /* CMP */
                0xc9 | 0xc5 | 0xd5 | 0xcd | 0xdd | 0xd9 | 0xc1 | 0xd1 => {
                    self.compare(&opcode.mode, self.register_a);
                }

                /* LSR */
                0x4A => {
                    self.lsr_accumulator();
                }
                0x46 | 0x56 | 0x4e | 0x5e => {
                    self.lsr(&opcode.mode);
                }

                /* INC */
                0xe6 | 0xf6 | 0xee | 0xfe => {
                    self.inc(&opcode.mode);
                }

                /* JMP */
                0x4c => {
                    let mem_address = self.mem_read_u16(self.program_counter);
                    self.program_counter = mem_address;
                }

                /* NOP */
                0xEA => {
                    // no operation
                }

                _ => todo!(),
            }

            if program_counter_state == self.program_counter {
                self.program_counter += (opcode.len - 1) as u16;
            }

            callback(self);
        }
    }

    fn update_zero_and_negative_flags(&mut self, result: u8) {
        if result == 0 {
            self.status |= CpuFlags::ZERO;
        } else {
            self.status &= !CpuFlags::ZERO;
        }

        if result & CpuFlags::NEGATIVE != 0 {
            self.status |= CpuFlags::NEGATIVE;
        } else {
            self.status &= !CpuFlags::NEGATIVE;
        }
    }

    fn set_register_a(&mut self, value: u8) {
        self.register_a = value;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn add_to_register_a(&mut self, data: u8) {
        let sum = self.register_a as u16
            + data as u16
            + (if self.status & CpuFlags::CARRY != 0 {
                1
            } else {
                0
            }) as u16;

        let carry = sum > 0xff;

        if carry {
            self.status |= CpuFlags::CARRY;
        } else {
            self.status &= !CpuFlags::CARRY;
        }

        let result = sum as u8;

        if (data ^ result) & (result ^ self.register_a) & 0x80 != 0 {
            self.status |= CpuFlags::OVERFLOW;
        } else {
            self.status &= !CpuFlags::OVERFLOW;
        }

         self.set_register_a(result);
    }

    fn compare(&mut self, mode: &AddressingMode, compare_with: u8) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        if data <= compare_with {
            self.status |= CpuFlags::CARRY;
        } else {
            self.status &= !CpuFlags::CARRY;
        }

        self.update_zero_and_negative_flags(compare_with.wrapping_sub(data));
    }

    fn branch(&mut self, condition: bool) {
        if condition {
            let jump: i8 = self.mem_read(self.program_counter) as i8;
            let jump_addr = self
                .program_counter
                .wrapping_add(1)
                .wrapping_add(jump as u16);

            self.program_counter = jump_addr;
        }
    }

    fn lda(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let value = self.mem_read(addr);
        self.set_register_a(value);
    }

    fn ldx(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_x = data;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn ldy(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.register_y = data;
        self.update_zero_and_negative_flags(self.register_y);
    }

    fn adc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let value = self.mem_read(addr);
        self.add_to_register_a(value);
    }

    fn sbc(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(&mode);
        let data = self.mem_read(addr);
        self.add_to_register_a(((data as i8).wrapping_neg().wrapping_sub(1)) as u8);
    }

    fn dex(&mut self) {
        self.register_x = self.register_x.wrapping_sub(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn dec(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_sub(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn sta(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        self.mem_write(addr, self.register_a);
    }

    fn tax(&mut self) {
        self.register_x = self.register_a;
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn txa(&mut self) {
        self.register_a = self.register_x;
        self.update_zero_and_negative_flags(self.register_a);
    }

    fn inx(&mut self) {
        self.register_x = self.register_x.wrapping_add(1);
        self.update_zero_and_negative_flags(self.register_x);
    }

    fn inc(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        data = data.wrapping_add(1);
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn brk(&mut self) {
        self.status = self.status | CpuFlags::BREAK | CpuFlags::BREAK2;
    }

    fn and(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        self.set_register_a(data & self.register_a);
    }

    fn lsr_accumulator(&mut self) {
        let mut data = self.register_a;
        if data & 1 == 1 {
            self.status |= CpuFlags::CARRY;
        } else {
            self.status &= !CpuFlags::CARRY;
        }
        data = data >> 1;
        self.set_register_a(data)
    }

    fn lsr(&mut self, mode: &AddressingMode) -> u8 {
        let addr = self.get_operand_address(mode);
        let mut data = self.mem_read(addr);
        if data & 1 == 1 {
            self.status |= CpuFlags::CARRY;
        } else {
            self.status &= !CpuFlags::CARRY;
        }
        data = data >> 1;
        self.mem_write(addr, data);
        self.update_zero_and_negative_flags(data);
        data
    }

    fn bit(&mut self, mode: &AddressingMode) {
        let addr = self.get_operand_address(mode);
        let data = self.mem_read(addr);
        let and = self.register_a & data;

        // Set ZERO flag
        if and == 0 {
            self.status |= CpuFlags::ZERO;
        } else {
            self.status &= !CpuFlags::ZERO;
        }

        // Set NEGATIV flag
        if data & 0b1000_0000 > 0 {
            self.status |= CpuFlags::NEGATIVE;
        } else {
            self.status &= !CpuFlags::NEGATIVE;
        }

        // Set OVERFLOW flag
        if data & 0b0100_0000 > 0 {
            self.status |= CpuFlags::OVERFLOW;
        } else {
            self.status &= !CpuFlags::OVERFLOW;
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_0xa9_lda_immediate_load_data() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);

        cpu.load(vec![0xa9, 0x05, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert_eq!(cpu.register_a, 0x05);
        assert!(cpu.status & 0b0000_0010 == 0b00);
        assert!(cpu.status & 0b1000_0000 == 0);
    }

    #[test]
    fn test_0xa9_lda_zero_flag() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);

        cpu.load(vec![0xa9, 0x00, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert!(cpu.status & 0b0000_0010 == 0b10);
    }

    #[test]
    fn test_0xaa_tax_move_a_to_x() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);
        cpu.register_a = 10;

        cpu.load(vec![0xa9, 0x0A, 0xaa, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert_eq!(cpu.register_x, 10)
    }

    #[test]
    fn test_5_ops_working_together() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);
        cpu.load(vec![0xa9, 0xc0, 0xaa, 0xe8, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert_eq!(cpu.register_x, 0xc1)
    }

    #[test]
    fn test_inx_overflow() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);
        cpu.register_x = 0xff;

        cpu.load(vec![0xa9, 0xff, 0xaa, 0xe8, 0xe8, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert_eq!(cpu.register_x, 1)
    }

    #[test]
    fn test_lda_from_memory() {
        let bus = Bus::new();
        let mut cpu = CPU::new(bus);
        cpu.mem_write(0x10, 0x55);

        cpu.load(vec![0xa5, 0x10, 0x00]);
        cpu.reset();
        cpu.program_counter = 0x0600;
        cpu.run();

        assert_eq!(cpu.register_a, 0x55);
    }
}
