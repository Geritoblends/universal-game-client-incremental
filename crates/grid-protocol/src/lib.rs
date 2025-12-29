use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GridCell {
    pub character: u32, // UTF-32 character
    pub fg_color: u8,   // ANSI 256 color index
    pub bg_color: u8,   // ANSI 256 color index
    pub padding: u16,   // Padding for alignment
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug, Default)]
pub struct GridInput {
    pub input_type: u32, // 0=None, 1=Key
    pub key_code: u32,   // UTF-32 char or Special Key Constant
    pub modifiers: u8,   // Bitmask: 1=Shift, 2=Ctrl, 4=Alt
    pub padding: [u8; 3],
}

// Input Types
pub const INPUT_NONE: u32 = 0;
pub const INPUT_KEY: u32 = 1;

// Special Key Constants (Starting after max valid Unicode 0x10FFFF)
pub const KEY_ENTER: u32 = 0x110000;
pub const KEY_ESC: u32 = 0x110001;
pub const KEY_BACKSPACE: u32 = 0x110002;
pub const KEY_LEFT: u32 = 0x110003;
pub const KEY_RIGHT: u32 = 0x110004;
pub const KEY_UP: u32 = 0x110005;
pub const KEY_DOWN: u32 = 0x110006;
pub const KEY_DELETE: u32 = 0x110007;
pub const KEY_TAB: u32 = 0x110008;

// Modifiers
pub const MOD_SHIFT: u8 = 1;
pub const MOD_CTRL: u8 = 2;
pub const MOD_ALT: u8 = 4;
