// Simple ANSI color library.
//
// TODO: Windows support.

const u8 color_black = 0u8;
const u8 color_red = 1u8;
const u8 color_green = 2u8;
const u8 color_yellow = 3u8;
const u8 color_blue = 4u8;
const u8 color_magenta = 5u8;
const u8 color_cyan = 6u8;
const u8 color_light_gray = 7u8;
const u8 color_light_grey = 7u8;
const u8 color_dark_gray = 8u8;
const u8 color_dark_grey = 8u8;
const u8 color_bright_red = 9u8;
const u8 color_bright_green = 10u8;
const u8 color_bright_yellow = 11u8;
const u8 color_bright_blue = 12u8;
const u8 color_bright_magenta = 13u8;
const u8 color_bright_cyan = 14u8;
const u8 color_bright_white = 15u8;

fn esc(io::buf_writer writer) {
    writer.write(vec(0x1bu8, '[' as u8));
}

fn reset(io::buf_writer writer) {
    esc(writer);
    writer.write(vec('0' as u8, 'm' as u8));
}

fn color_supported() -> bool {
    ret generic_os::getenv("TERM") == option::some[str]("xterm-color");
}

fn set_color(io::buf_writer writer, u8 first_char, u8 color) {
    assert (color < 16u8);

    esc(writer);
    if (color >= 8u8) {
        writer.write(vec('1' as u8, ';' as u8));
        color -= 8u8;
    }
    writer.write(vec(first_char, ('0' as u8) + color, 'm' as u8));
}

fn fg(io::buf_writer writer, u8 color) {
    ret set_color(writer, '3' as u8, color);
}

fn bg(io::buf_writer writer, u8 color) {
    ret set_color(writer, '4' as u8, color);
}

// export fg;
// export bg;

