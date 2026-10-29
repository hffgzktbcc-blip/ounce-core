//! Shared reply helpers and routing for the libnx-shaped system services (`applet`, `time`, `set`,
//! `psm`, `apm`).
//!
//! The harness stages talk to some of these services with bare CMIF frames (no HIPC header, so the
//! request type is 0) and read fixed offsets; that inline form keeps the older handlers in `ipc.rs`.
//! A request with a real HIPC header (type `Request` / `RequestWithContext`, which libnx always sends)
//! takes the command tables in the per-service modules, which follow libnx's public command IDs.

use super::hipc::{TYPE_REQUEST, TYPE_REQUEST_WITH_CONTEXT};
use super::ipc::{IpcManager, ServiceType, RESULT_UNKNOWN_COMMAND};
use crate::mmu::Memory;

impl IpcManager {
    /// The current request carries a HIPC header, as libnx sends it.
    pub(super) fn is_libnx_request(&self) -> bool {
        matches!(self.request.kind, TYPE_REQUEST | TYPE_REQUEST_WITH_CONTEXT)
    }

    /// Routes `applet`, `time`, `set`, `psm` and `apm` requests. `None` leaves the request to `ipc.rs`.
    pub(super) fn handle_system_service(
        &mut self,
        service: &ServiceType,
        cmd: u32,
        args: u64,
        tls: u64,
        mmu: &mut Memory,
    ) -> Option<u64> {
        use super::applet::AppletInterface as A;
        use ServiceType as S;
        let libnx = self.is_libnx_request();
        Some(match service {
            // Services the harness also drives inline: only libnx-shaped requests come here.
            S::AppletOE if libnx => self.handle_applet(A::OeRoot, cmd, args, tls, mmu),
            S::ApplicationProxy if libnx => {
                self.handle_applet(A::Proxy(super::applet::ProxyKind::Application), cmd, args, tls, mmu)
            }
            S::CommonStateGetter if libnx => self.handle_applet(A::CommonStateGetter, cmd, args, tls, mmu),
            S::SelfController if libnx => self.handle_applet(A::SelfController, cmd, args, tls, mmu),
            S::ApplicationFunctions if libnx => self.handle_applet(A::ApplicationFunctions, cmd, args, tls, mmu),
            S::SetSys if libnx => self.handle_setsys(cmd, args, tls, mmu),
            S::Time if libnx => self.handle_time(cmd, tls, mmu),
            S::SystemClock if libnx => self.handle_system_clock(cmd, tls, mmu),
            S::SteadyClock if libnx => self.handle_steady_clock(cmd, tls, mmu),
            // Services only libnx-shaped code reaches.
            S::AppletAE => self.handle_applet(A::AeRoot, cmd, args, tls, mmu),
            S::Applet(iface) => self.handle_applet(*iface, cmd, args, tls, mmu),
            S::Set => self.handle_set(cmd, args, tls, mmu),
            S::TimeZoneService => self.handle_time_zone(cmd, args, tls, mmu),
            S::Psm => self.handle_psm(cmd, tls, mmu),
            S::PsmSession => self.handle_psm_session(cmd, tls, mmu),
            S::Apm => self.handle_apm(cmd, tls, mmu),
            S::ApmSession => self.handle_apm_session(cmd, args, tls, mmu),
            S::ServiceEvent(_) => 0xE401,
            _ => return None,
        })
    }

    /// Success with raw output data.
    pub(super) fn reply_data(&self, tls: u64, mmu: &mut Memory, data: &[u8]) -> u64 {
        self.reply(tls, mmu, 0, data, &[], None);
        0
    }

    /// Success with no output.
    pub(super) fn reply_ok(&self, tls: u64, mmu: &mut Memory) -> u64 {
        self.reply_data(tls, mmu, &[])
    }

    /// A failure result with no output.
    pub(super) fn reply_error(&self, tls: u64, mmu: &mut Memory, result: u32) -> u64 {
        self.reply(tls, mmu, result, &[], &[], None);
        0
    }

    /// Opens a sub-interface: a move handle, or an out object on a domain.
    pub(super) fn reply_object(&mut self, tls: u64, mmu: &mut Memory, service: ServiceType) -> u64 {
        let h = self.allocate_handle(service);
        self.reply(tls, mmu, 0, &[], &[], Some(h));
        0
    }

    /// Hands out an event as a copy handle.
    pub(super) fn reply_event(&self, tls: u64, mmu: &mut Memory, handle: u32) -> u64 {
        self.reply(tls, mmu, 0, &[], &[handle], None);
        0
    }

    /// A command of a known interface this model does not implement: report it, don't fake success.
    pub(super) fn reply_unimplemented(&self, iface: &str, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        eprintln!("[HIPC] {iface} command {cmd} not implemented");
        self.reply_error(tls, mmu, RESULT_UNKNOWN_COMMAND)
    }

    /// Writes `data` to the request's first non-empty `B` buffer, or receive-list entry,
    /// truncated to the buffer size. Returns the bytes written.
    pub(super) fn write_out_buffer(&self, mmu: &mut Memory, data: &[u8]) -> usize {
        let Some(buf) = self.request.recv_buffers.iter().find(|b| b.size > 0).or_else(|| self.request.recv_list.iter().find(|b| b.size > 0)) else {
            return 0;
        };
        let n = data.len().min(buf.size as usize);
        if n == 0 || mmu.write_bytes(buf.addr, &data[..n]).is_err() {
            return 0;
        }
        n
    }

    /// State of an event handed out by a service: `Some(signaled)` for those events, `None` otherwise.
    /// The applet message event is signaled while the message queue holds a message.
    pub fn service_event_signaled(&self, handle: u32) -> Option<bool> {
        match self.handles.get(&handle) {
            Some(ServiceType::ServiceEvent("vi-binder")) => Some(true),
            Some(ServiceType::ServiceEvent(_)) => Some(self.applet.message_event == Some(handle) && !self.applet.messages.is_empty()),
            _ => None,
        }
    }
}

/// Test helpers that build requests the way libnx's `serviceDispatch` lays them out.
#[cfg(test)]
pub(super) mod test_util {
    use super::*;
    use crate::audio::AudioOutServer;
    use crate::fs::VirtualFileSystem;
    use crate::gpu::nvgpu::NvgpuChannel;
    use crate::kernel::ipc::{CMIF_IN_MAGIC, CMIF_OUT_MAGIC};
    use crate::mmu::Permission;

    pub const TLS: u64 = 0x8000_0000;
    pub const BUF: u64 = 0x8001_0000;

    /// A request: command, raw input data, and optional buffers.
    #[derive(Default)]
    pub struct Req<'a> {
        pub cmd: u32,
        pub data: &'a [u8],
        pub pid: bool,
        pub copy_handles: &'a [u32],
        /// `A` (send) buffer: (addr, size).
        pub send: Option<(u64, u64)>,
        /// `B` (receive, map alias) buffer.
        pub recv: Option<(u64, u64)>,
        /// `C` (receive list, pointer) buffer.
        pub recv_list: Option<(u64, u64)>,
        /// Domain object ID, for requests on a domain session.
        pub domain_object: Option<u32>,
        /// `X` (send static, pointer) buffer at index 0.
        pub send_static: Option<(u64, u64)>,
    }

    pub struct Harness {
        pub mmu: Memory,
        pub ipc: IpcManager,
        nvgpu: NvgpuChannel,
        audio: AudioOutServer,
        vfs: VirtualFileSystem,
    }

    /// Parsed reply.
    #[derive(Debug)]
    pub struct Reply {
        pub result: u32,
        pub data: Vec<u8>,
        pub copy_handles: Vec<u32>,
        pub move_handles: Vec<u32>,
        pub objects: Vec<u32>,
    }

    impl Reply {
        pub fn u8(&self, off: usize) -> u8 {
            self.data[off]
        }
        pub fn u32(&self, off: usize) -> u32 {
            u32::from_le_bytes(self.data[off..off + 4].try_into().unwrap())
        }
        pub fn u64(&self, off: usize) -> u64 {
            u64::from_le_bytes(self.data[off..off + 8].try_into().unwrap())
        }
        /// The session this reply opened (move handle or domain object).
        pub fn object(&self) -> u32 {
            *self.move_handles.first().or(self.objects.first()).expect("reply opened no object")
        }
    }

    impl Harness {
        pub fn new() -> Self {
            let mut mmu = Memory::new();
            mmu.map_range(TLS, 0x1000, Permission::READ_WRITE);
            mmu.map_range(BUF, 0x10000, Permission::READ_WRITE);
            Self { mmu, ipc: IpcManager::new(), nvgpu: NvgpuChannel::new(), audio: AudioOutServer::new(), vfs: VirtualFileSystem::new() }
        }

        pub fn call(&mut self, session: u32, req: Req) -> Reply {
            self.call_out(session, req, 0)
        }

        /// Sends `req` and parses a reply with `out_len` bytes of output data.
        pub fn call_out(&mut self, session: u32, req: Req, out_len: usize) -> Reply {
            let mmu = &mut self.mmu;
            for i in 0..0x100 {
                mmu.write_u32(TLS + i * 4, 0).unwrap();
            }
            let num_send = req.send.is_some() as u32;
            let num_recv = req.recv.is_some() as u32;
            let special = req.pid || !req.copy_handles.is_empty();
            let mut off = 8u64;
            if special {
                mmu.write_u32(TLS + off, req.pid as u32 | (req.copy_handles.len() as u32) << 1).unwrap();
                off += 4;
                if req.pid {
                    mmu.write_u64(TLS + off, 0x51).unwrap();
                    off += 8;
                }
                for &h in req.copy_handles {
                    mmu.write_u32(TLS + off, h).unwrap();
                    off += 4;
                }
            }
            if let Some((addr, size)) = req.send_static {
                mmu.write_u32(TLS + off, ((addr >> 36) as u32 & 0x3F) << 6 | ((addr >> 32) as u32 & 0xF) << 12 | (size as u32) << 16).unwrap();
                mmu.write_u32(TLS + off + 4, addr as u32).unwrap();
                off += 8;
            }
            for (addr, size) in req.send.iter().chain(req.recv.iter()) {
                mmu.write_u32(TLS + off, *size as u32).unwrap();
                mmu.write_u32(TLS + off + 4, *addr as u32).unwrap();
                mmu.write_u32(TLS + off + 8, ((addr >> 32) as u32 & 0xF) << 28).unwrap();
                off += 12;
            }
            let cmif_start = (off + 15) & !15;
            let mut cmif = cmif_start;
            if let Some(object) = req.domain_object {
                let data_size = 16 + req.data.len() as u32;
                mmu.write_u32(TLS + cmif, 1 | data_size << 16).unwrap();
                mmu.write_u32(TLS + cmif + 4, object).unwrap();
                cmif += 16;
            }
            mmu.write_u32(TLS + cmif, CMIF_IN_MAGIC).unwrap();
            mmu.write_u32(TLS + cmif + 8, req.cmd).unwrap();
            mmu.write_bytes(TLS + cmif + 16, req.data).unwrap();
            let end = cmif + 16 + req.data.len() as u64;
            let num_words = ((end - off).div_ceil(4) + 4) as u32; // + padding words, as libnx reserves
            let recv_mode = if req.recv_list.is_some() { 2 } else { 0 };
            let num_statics = req.send_static.is_some() as u32;
            mmu.write_u32(TLS, 4 | num_statics << 16 | num_send << 20 | num_recv << 24).unwrap();
            mmu.write_u32(TLS + 4, num_words | recv_mode << 10 | (special as u32) << 31).unwrap();
            if let Some((addr, size)) = req.recv_list {
                let roff = off + num_words as u64 * 4;
                mmu.write_u32(TLS + roff, addr as u32).unwrap();
                mmu.write_u32(TLS + roff + 4, (addr >> 32) as u32 & 0xFFFF | (size as u32) << 16).unwrap();
            }

            let session_handle = session;
            let rc = self.ipc.handle_sync_request(session_handle, TLS, &mut self.mmu, &mut self.nvgpu, &mut self.audio, &mut self.vfs);
            assert_eq!(rc, 0, "transport error");

            let mmu = &self.mmu;
            let w1 = mmu.read_u32(TLS + 4).unwrap();
            let mut off = 8u64;
            let (mut copy_handles, mut move_handles) = (Vec::new(), Vec::new());
            if w1 >> 31 != 0 {
                let sh = mmu.read_u32(TLS + 8).unwrap();
                off += 4;
                for _ in 0..(sh >> 1) & 0xF {
                    copy_handles.push(mmu.read_u32(TLS + off).unwrap());
                    off += 4;
                }
                for _ in 0..(sh >> 5) & 0xF {
                    move_handles.push(mmu.read_u32(TLS + off).unwrap());
                    off += 4;
                }
            }
            let mut cmif = (off + 15) & !15;
            let num_objects = if req.domain_object.is_some() {
                let n = mmu.read_u32(TLS + cmif).unwrap() as usize;
                cmif += 16;
                n
            } else {
                0
            };
            assert_eq!(mmu.read_u32(TLS + cmif).unwrap(), CMIF_OUT_MAGIC);
            let result = mmu.read_u32(TLS + cmif + 8).unwrap();
            let mut data = vec![0u8; out_len];
            mmu.read_bytes(TLS + cmif + 16, &mut data).unwrap();
            let objects =
                (0..num_objects).map(|i| mmu.read_u32(TLS + cmif + 16 + out_len as u64 + i as u64 * 4).unwrap()).collect();
            Reply { result, data, copy_handles, move_handles, objects }
        }

        /// `sm:` GetServiceHandle(name) on a fresh `sm:` session.
        pub fn get_service(&mut self, name: &str) -> u32 {
            let sm = self.ipc.allocate_handle(ServiceType::Sm);
            let mut n = [0u8; 8];
            n[..name.len()].copy_from_slice(name.as_bytes());
            let r = self.call(sm, Req { cmd: 1, data: &n, ..Default::default() });
            assert_eq!(r.result, 0);
            r.object()
        }

        pub fn read_buf(&self, addr: u64, len: usize) -> Vec<u8> {
            let mut v = vec![0u8; len];
            self.mmu.read_bytes(addr, &mut v).unwrap();
            v
        }
    }
}
