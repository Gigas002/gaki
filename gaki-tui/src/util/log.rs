macro_rules! log{
    ($($elem: expr),+ ) => {
        if !std::io::IsTerminal::is_terminal(&std::io::stderr()) {
            eprintln!(concat!($(concat!(stringify!($elem), " - {:?}\n")),+), $($elem),+);
        }
    };
}

pub(crate) use log;
