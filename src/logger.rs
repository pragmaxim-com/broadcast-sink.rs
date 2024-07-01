use chrono::Local;
use std::fmt;

pub fn error(args: fmt::Arguments) {
    let now = Local::now();
    println!("[{}] ERROR {}", now.format("%Y-%m-%d %H:%M:%S"), args);
}

#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {
        $crate::logger::error(format_args!($($arg)*))
    };
}
