#[cfg(target_family = "unix")]
pub use crate::os::unix::flags::*;

#[cfg(target_os = "linux")]
pub use crate::os::linux::flags::*;

#[cfg(target_os = "android")]
pub use crate::os::android::flags::*;

#[cfg(target_vendor = "apple")]
pub use crate::os::darwin::flags::*;

#[cfg(target_os = "windows")]
pub use crate::os::windows::flags::*;

#[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
pub use crate::os::bsd::flags::*;

#[cfg(target_os = "hermit")]
mod hermit_flags {
    use crate::interface::interface::Interface;
    pub const IFF_UP: i32 = 0x1;
    pub const IFF_BROADCAST: i32 = 0x2;
    pub const IFF_LOOPBACK: i32 = 0x8;
    pub const IFF_POINTOPOINT: i32 = 0x10;
    pub const IFF_RUNNING: i32 = 0x40;
    pub const IFF_MULTICAST: i32 = 0x1000;
    pub fn is_running(interface: &Interface) -> bool {
        interface.flags & (IFF_RUNNING as u32) != 0
    }
    pub fn is_physical_interface(_interface: &Interface) -> bool { false }
}
#[cfg(target_os = "hermit")]
pub use hermit_flags::*;
