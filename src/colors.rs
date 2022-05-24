use termcolor::ColorSpec;

use once_cell::sync::Lazy;

#[macro_export]
macro_rules! cprintln {
    ($($rest:tt)*) => {
        {
            use std::io::Write;
            #[allow(unused_imports)]
            use termcolor::{ColorSpec, WriteColor};
            let bufwtr = termcolor::BufferWriter::stdout(*$crate::colors::STDOUT_COLOR_CHOICE);
            let mut buffer = bufwtr.buffer();
            $crate::format_colors!(buffer @ $($rest)*);
            let _ = write!(&mut buffer, "\n");
            let _ = bufwtr.print(&buffer);
        }
    };
}

#[macro_export]
macro_rules! cprint {
    ($($rest:tt)*) => {
        {
            use std::io::Write;
            #[allow(unused_imports)]
            use termcolor::{ColorSpec,  WriteColor};
            let bufwtr = termcolor::BufferWriter::stdout(*$crate::colors::STDOUT_COLOR_CHOICE);
            let mut buffer = bufwtr.buffer();
            $crate::format_colors!(buffer @ $($rest)*);
            let _ = bufwtr.print(&buffer);
        }
    };
}

#[macro_export]
macro_rules! ecprintln {
    // () => {
    //     println!()
    // };
    ($($rest:tt)*) => {
        {
            use std::io::Write;
            #[allow(unused_imports)]
            use termcolor::{ColorSpec, WriteColor};
            let bufwtr = termcolor::BufferWriter::stderr(*$crate::colors::STDERR_COLOR_CHOICE);
            let mut buffer = bufwtr.buffer();
            $crate::format_colors!(buffer @ $($rest)*);
            let _ = write!(&mut buffer, "\n");
            let _ = bufwtr.print(&buffer);
        }
    };
}

#[macro_export]
macro_rules! ecprint {
    ($($rest:tt)*) => {
        {
            use std::io::Write;
            use termcolor::{ColorSpec, WriteColor};
            let bufwtr = termcolor::BufferWriter::stderr(*$crate::colors::STDERR_COLOR_CHOICE);
            let mut buffer = bufwtr.buffer();
            $crate::format_colors!(buffer @ $($rest)*);
            let _ = bufwtr.print(&buffer);
        }
    };
}

#[macro_export]
#[doc(hidden)]
macro_rules! format_colors {
    ($buffer:ident @ $fmt:literal $($color:ident)+ $(, $($rest:tt)* )? ) => {
        let mut color = ColorSpec::new();
        $(
            $crate::colors::add_to_color_spec(&mut color, $crate::colors::ColoringOption::$color);
        )+
        let _ = $buffer.set_color(&color);
        let _ = write!(&mut $buffer, $fmt);
        let _ = $buffer.reset();
        $crate::format_colors!($buffer @ $($($rest)*)?)
    };
    ($buffer:ident @ $fmt:literal $(, $($rest:tt)* )? ) => {
        let _ = write!(&mut $buffer, $fmt);
        $crate::format_colors!($buffer @ $($($rest)*)?)
    };
    ($buffer:ident @ ) => {};
}

pub static STDOUT_COLOR_CHOICE: Lazy<termcolor::ColorChoice>  = Lazy::new(|| {
    if atty::is(atty::Stream::Stdout) {
        termcolor::ColorChoice::Auto
    } else {
        termcolor::ColorChoice::Never
    }
});

pub static STDERR_COLOR_CHOICE: Lazy<termcolor::ColorChoice>  = Lazy::new(|| {
    if atty::is(atty::Stream::Stderr) {
        termcolor::ColorChoice::Auto
    } else {
        termcolor::ColorChoice::Never
    }
});

#[allow(non_camel_case_types, dead_code)]
#[doc(hidden)]
pub enum ColoringOption {
    bold,
    italic,
    dimmed,
    underline,
    intense,
    black,
    blue,
    green,
    red,
    cyan,
    magenta,
    yellow,
    white,
    on_black,
    on_blue,
    on_green,
    on_red,
    on_cyan,
    on_magenta,
    on_yellow,
    on_white,
}

#[doc(hidden)]
pub fn add_to_color_spec(spec: &mut ColorSpec, option: ColoringOption) -> &mut ColorSpec {
    match option {
        ColoringOption::bold => spec.set_bold(true),
        ColoringOption::italic => spec.set_italic(true),
        ColoringOption::dimmed => spec.set_dimmed(true),
        ColoringOption::underline => spec.set_underline(true),
        ColoringOption::intense => spec.set_intense(true),
        ColoringOption::black => spec.set_fg(Some(termcolor::Color::Black)),
        ColoringOption::blue => spec.set_fg(Some(termcolor::Color::Blue)),
        ColoringOption::green => spec.set_fg(Some(termcolor::Color::Green)),
        ColoringOption::red => spec.set_fg(Some(termcolor::Color::Red)),
        ColoringOption::cyan => spec.set_fg(Some(termcolor::Color::Cyan)),
        ColoringOption::magenta => spec.set_fg(Some(termcolor::Color::Magenta)),
        ColoringOption::yellow => spec.set_bg(Some(termcolor::Color::Yellow)),
        ColoringOption::white => spec.set_bg(Some(termcolor::Color::White)),
        ColoringOption::on_black => spec.set_bg(Some(termcolor::Color::Black)),
        ColoringOption::on_blue => spec.set_bg(Some(termcolor::Color::Blue)),
        ColoringOption::on_green => spec.set_bg(Some(termcolor::Color::Green)),
        ColoringOption::on_red => spec.set_bg(Some(termcolor::Color::Red)),
        ColoringOption::on_cyan => spec.set_bg(Some(termcolor::Color::Cyan)),
        ColoringOption::on_magenta => spec.set_bg(Some(termcolor::Color::Magenta)),
        ColoringOption::on_yellow => spec.set_bg(Some(termcolor::Color::Yellow)),
        ColoringOption::on_white => spec.set_bg(Some(termcolor::Color::White)),
    }
}
