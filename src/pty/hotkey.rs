pub(crate) fn is_wrapper_hotkey(byte: u8) -> bool {
    byte == 0x02 || byte == 0x05 || byte == 0x13 || byte == 0x11
}
