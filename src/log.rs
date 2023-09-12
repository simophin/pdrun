macro_rules! logPrint {
    ($target:expr, $fmt:tt $(,$arg:expr)*) => {
        let message = format!("[{}]: {}", $target, format!($fmt $(,$arg)*));
        println!("{message}");
    };
}

macro_rules! elogPrint {
    ($target:expr, $fmt:tt $(,$arg:expr)*) => {
        let message = format!("[{}]: {}", $target, format!($fmt $(,$arg)*));
        eprintln!("{message}");
    };
}

pub(crate) use elogPrint;
pub(crate) use logPrint;
