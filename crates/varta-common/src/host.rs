//! Canonical operation names and Wasm signatures shared by compiler and runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum HostOperation {
    LogInt,
    LogString,
    Time,
    EnvHas,
    EnvGet,
    EnvPrint,
    FsRead,
    FsPrint,
    NetFetch,
    NetGet,
    Exit,
}
impl HostOperation {
    pub fn name(self) -> &'static str {
        match self {
            Self::LogInt => "log_i32",
            Self::LogString => "log_string",
            Self::Time => "time_now",
            Self::EnvHas => "env_has",
            Self::EnvGet => "env_get",
            Self::EnvPrint => "env_print",
            Self::FsRead => "fs_read",
            Self::FsPrint => "fs_print",
            Self::NetFetch => "net_fetch",
            Self::NetGet => "net_get",
            Self::Exit => "process_exit",
        }
    }
    pub fn signature(self) -> (usize, usize) {
        match self {
            Self::LogInt | Self::Exit => (1, 0),
            Self::LogString | Self::EnvPrint | Self::FsPrint => (2, 0),
            Self::Time => (0, 1),
            _ => (2, 1),
        }
    }
}

impl HostOperation {
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "log_i32" => Self::LogInt,
            "log_string" => Self::LogString,
            "time_now" => Self::Time,
            "env_has" => Self::EnvHas,
            "env_get" => Self::EnvGet,
            "env_print" => Self::EnvPrint,
            "fs_read" => Self::FsRead,
            "fs_print" => Self::FsPrint,
            "net_fetch" => Self::NetFetch,
            "net_get" => Self::NetGet,
            "process_exit" => Self::Exit,
            _ => return None,
        })
    }
    #[must_use]
    pub fn capability(self) -> Option<&'static str> {
        Some(match self {
            Self::LogInt | Self::LogString => return None,
            Self::Time => "time.now",
            Self::EnvHas => "env.has",
            Self::EnvGet => "env.get",
            Self::EnvPrint => "env.print",
            Self::FsRead => "fs.read",
            Self::FsPrint => "fs.print",
            Self::NetFetch => "net.fetch",
            Self::NetGet => "net.get",
            Self::Exit => "process.exit",
        })
    }
}
