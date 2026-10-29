#![allow(dead_code)]

use crate::audio::AudioOutServer;
use crate::fs::VirtualFileSystem;
use crate::gpu::nvgpu::{NvgpuChannel, NVGPU_IOCTL_CHANNEL_SUBMIT_GPFIFO};
use crate::mmu::Memory;
use std::collections::HashMap;

pub const CMIF_IN_MAGIC: u32 = 0x49434653;  // "SFCI"
pub const CMIF_OUT_MAGIC: u32 = 0x4F434653; // "SFCO"
pub const DEFAULT_TLS_ADDR: u64 = 0x7FFFE00000;
pub const TLS_SIZE: usize = 0x1000;

/// Horizon OS System Service Types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ServiceType {
    /// Service Manager ("sm:")
    Sm,
    /// Nvidia GPU Driver ("nvdrv" / "nvdrv:a")
    Nvdrv,
    /// Video Interface ("vi:u" / "vi:m" / "vi:s")
    Vi,
    /// Application Display Service ("IApplicationDisplayService")
    ApplicationDisplayService,
    ViBinderRelay,
    ViSystemDisplay,
    ViManagerDisplay,
    /// Human Interface Devices ("hid")
    Hid,
    /// Applet Resource ("IAppletResource")
    AppletResource,
    /// Filesystem Proxy ("fsp-srv")
    FspSrv,
    /// Storage Data Proxy ("IStorage")
    Storage,
    /// Filesystem Interface ("IFileSystem")
    FileSystem(String),
    /// Directory Enumerator ("IDirectory")
    Directory {
        path: String,
        entries: Vec<String>,
        index: usize,
    },
    /// File Interface ("IFile")
    File(u32),
    /// Audio Out ("audout:u")
    AudOut,
    /// Audio Renderer ("audren:u")
    AudRen,
    /// Applet OE Root ("appletOE")
    AppletOE,
    /// Application Proxy ("IApplicationProxy")
    ApplicationProxy,
    /// Common State Getter ("ICommonStateGetter")
    CommonStateGetter,
    /// Self Controller ("ISelfController")
    SelfController,
    /// Application Functions ("IApplicationFunctions")
    ApplicationFunctions,
    /// System Settings ("set:sys" / "set")
    SetSys,
    /// Relocatable Objects / Dynamic Linker ("ro:1" / "ro:dmnt" / "ldr:ro")
    Ro,
    /// Shared Font Service ("pl:u" / "pl:s")
    Pl,
    /// Standard System Time Service ("time:u" / "time:s")
    Time,
    /// System Clock Interface ("ISystemClock")
    SystemClock,
    /// System Steady Clock ("ISteadyClock")
    SteadyClock,
    /// Play Report Service ("prepo:u" / "prepo:a")
    Prepo,
    /// Background Content Access ("bcat:u" / "bcat:a")
    Bcat,
    /// Applet Manager for system and library applets ("appletAE")
    AppletAE,
    /// An `am` sub-interface (`kernel/applet.rs`)
    Applet(super::applet::AppletInterface),
    /// Device settings ("set")
    Set,
    /// `ITimeZoneService` from `time:u` / `time:s`
    TimeZoneService,
    /// Power supply ("psm") and its `IPsmSession`
    Psm,
    PsmSession,
    /// Performance management ("apm") and its `ISession`
    Apm,
    ApmSession,
    /// Readable event handed out by a system service (applet message queue, psm, ...).
    ServiceEvent(&'static str),
    /// Generic stubbed service
    Generic(String),
    /// Shared memory object (not a session): handed out as a copy handle, mapped with svcMapSharedMemory.
    SharedMemory { size: u64 },
    /// Transfer memory object from svcCreateTransferMemory: guest memory lent to a service.
    TransferMemory { addr: u64, size: u64 },
    /// Readable event handed out by nvdrv `QueryEvent` (syncpoint and channel error events).
    NvEvent { fd: u32, event_id: u32 },
    /// `IAudioRenderer` opened through `audren:u` (state in `IpcManager::audio_renderer`).
    AudioRenderer,
    /// The audio renderer's system event: signaled once per rendered frame.
    AudioRendererEvent,
}

/// Size of the HID shared memory block libnx maps (`0x40000`).
pub const HID_SHARED_MEMORY_SIZE: u64 = 0x40000;
/// `sf::cmif::ResultUnknownCommandId` (module 10, description 221): the reply for commands of services
/// this model does not implement.
pub const RESULT_UNKNOWN_COMMAND: u32 = 221 << 9 | 10;
/// CMIF pointer buffer size reported to `QueryPointerBufferSize`.
pub const POINTER_BUFFER_SIZE: u32 = 0x8000;

pub const SHARED_FONT_ADDR: u64 = 0x1800_0000;
pub const SHARED_FONT_SIZE: usize = 0x10_0000; // 1 MB Shared Font Buffer

/// Reads a null-terminated string from CMIF arguments or a guest pointer.
fn read_cmif_string(mmu: &Memory, args_base: u64, max_len: usize) -> String {
    let maybe_ptr = mmu.read_u64(args_base).unwrap_or(0);
    if maybe_ptr >= 0x10000 && maybe_ptr < 0x0001_0000_0000_0000 && mmu.is_mapped(maybe_ptr) {
        let mut buf = vec![0u8; max_len];
        if mmu.read_bytes(maybe_ptr, &mut buf).is_ok() {
            let len = buf.iter().position(|&b| b == 0).unwrap_or(max_len);
            let s = String::from_utf8_lossy(&buf[..len]).trim().to_string();
            if !s.is_empty() {
                return s;
            }
        }
    }
    let mut buf = vec![0u8; max_len];
    if mmu.read_bytes(args_base, &mut buf).is_ok() {
        let len = buf.iter().position(|&b| b == 0).unwrap_or(max_len);
        String::from_utf8_lossy(&buf[..len]).trim().to_string()
    } else {
        String::new()
    }
}

#[derive(Default)]
struct ViBufferQueue {
    connected: bool,
    configured: u64,
    dequeued: Option<u8>,
    next_slot: u8,
}

impl ViBufferQueue {
    fn next_configured_slot(&mut self) -> Option<u8> {
        for delta in 0..64u8 {
            let slot = self.next_slot.wrapping_add(delta) & 63;
            if self.configured & (1u64 << slot) != 0 {
                self.next_slot = slot.wrapping_add(1) & 63;
                return Some(slot);
            }
        }
        None
    }

    fn available(&self) -> bool {
        self.connected && self.configured != 0 && self.dequeued.is_none()
    }
}

fn append_vi_buffer_output(out: &mut Vec<u8>, pending: u32) {
    for value in [1280u32, 720, 0, pending] {
        out.extend_from_slice(&value.to_le_bytes());
    }
}

/// Returns the command-specific bytes after an Android Parcel's interface token.
fn read_binder_args(mmu: &Memory, addr: u64, size: u64) -> Option<Vec<u8>> {
    if size < 24 { return None; }
    let payload_size = mmu.read_u32(addr).ok()? as u64;
    let payload_off = mmu.read_u32(addr + 4).ok()? as u64;
    let payload_end = payload_off.checked_add(payload_size)?;
    if payload_off < 16 || payload_end > size || payload_size < 8 { return None; }
    let string_len = mmu.read_u32(addr + payload_off + 4).ok()? as u64;
    let string_bytes = string_len.checked_add(1)?.checked_mul(2)?;
    let token_size = 8u64.checked_add(string_bytes.checked_add(3)? & !3)?;
    if token_size > payload_size { return None; }
    let mut command = vec![0; usize::try_from(payload_size - token_size).ok()?];
    mmu.read_bytes(addr + payload_off + token_size, &mut command).ok()?;
    Some(command)
}

/// Manages Horizon OS IPC handles and routes HIPC/CMIF messages to HLE services.
pub struct IpcManager {
    pub handles: HashMap<u32, ServiceType>,
    /// The request being handled, parsed from the HIPC header (PID, handles, buffers).
    pub request: super::hipc::Request,
    /// Process ID clients sent with `sm:` RegisterClient, if any.
    pub registered_pid: Option<u64>,
    /// Sessions converted to CMIF domains. Domain object IDs are entries in `handles`.
    pub domains: std::collections::HashSet<u32>,
    /// The current request arrived on a domain: replies use the domain layout.
    domain_reply: bool,
    /// `nvdrv` devices and nvmap objects for libnx-shaped requests.
    pub nvdrv: super::nvdrv::NvDrv,
    /// HID shared memory in libnx's layout; the host's controller input goes here.
    pub hid: super::hid::HidSharedMemory,
    /// The `IAudioRenderer` opened through `audren:u`, if any.
    pub audio_renderer: Option<super::audren::AudioRendererModel>,
    /// Applet message queue, focus and operation mode shared by every `am` session.
    pub applet: super::applet::AppletState,
    pub apm: super::apm::ApmState,
    pub time: super::time::TimeState,
    /// Minimal state for the VI layer's modeled Android graphic-buffer producer.
    vi_buffer_queue: ViBufferQueue,
    next_handle: u32,
    pub total_ipc_requests: u64,
    pub start_instant: std::time::Instant,
}

impl Default for IpcManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IpcManager {
    pub fn new() -> Self {
        Self {
            handles: HashMap::new(),
            request: Default::default(),
            registered_pid: None,
            domains: Default::default(),
            domain_reply: false,
            nvdrv: super::nvdrv::NvDrv::new(),
            hid: Default::default(),
            audio_renderer: None,
            applet: Default::default(),
            apm: Default::default(),
            time: Default::default(),
            vi_buffer_queue: Default::default(),
            next_handle: 0x10001,
            total_ipc_requests: 0,
            start_instant: std::time::Instant::now(),
        }
    }

    pub fn allocate_handle(&mut self, service: ServiceType) -> u32 {
        let h = self.next_handle;
        self.next_handle += 1;
        self.handles.insert(h, service);
        h
    }

    pub fn close_handle(&mut self, handle: u32) -> bool {
        self.handles.remove(&handle).is_some()
    }

    pub(super) fn vi_buffer_available(&self) -> bool {
        self.vi_buffer_queue.available()
    }

    /// Processes a synchronous HIPC/CMIF request located at `tls_addr` for `session_handle`.
    pub fn handle_sync_request(
        &mut self,
        session_handle: u32,
        tls_addr: u64,
        mmu: &mut Memory,
        nvgpu: &mut NvgpuChannel,
        audio: &mut AudioOutServer,
        vfs: &mut VirtualFileSystem,
    ) -> u64 {
        self.total_ipc_requests += 1;

        let mut service = match self.handles.get(&session_handle).cloned() {
            Some(s) => s,
            None => return 0xF601, // Invalid handle result code
        };
        self.domain_reply = false;

        // Check for Switch 2 Tipc (Tiny IPC) commands on `sm:` (used by native Switch 2 RO/FS modules on FW 23.0.1)
        let tipc_hdr = mmu.read_u32(tls_addr).unwrap_or(0);
        if service == ServiceType::Sm && (tipc_hdr == 0x10 || tipc_hdr == 0x12) {
            if tipc_hdr == 0x10 {
                // Tipc 0x10: RegisterClient -> p[2] = 0 (ResultSuccess)
                let _ = mmu.write_u32(tls_addr + 8, 0);
                return 0;
            } else {
                // Tipc 0x12: RegisterService(space, name, max_sessions)
                let mut name_bytes = [0u8; 8];
                let _ = mmu.read_bytes(tls_addr + 12, &mut name_bytes);
                let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
                let srv_name = String::from_utf8_lossy(&name_bytes[..len]).to_string();
                let port_handle = self.allocate_handle(ServiceType::Generic(srv_name));

                let _ = mmu.write_u32(tls_addr, 0x12);
                let _ = mmu.write_u32(tls_addr + 4, 0x8000_0001);
                let _ = mmu.write_u32(tls_addr + 8, 0x20);
                let _ = mmu.write_u32(tls_addr + 12, port_handle);
                let _ = mmu.write_u32(tls_addr + 16, 0); // ResultSuccess
                return 0;
            }
        }

        // CMIF payload: the HIPC raw data aligned to 16 bytes (TLS + 0x10 when there is no special header).
        self.request = super::hipc::parse_request(mmu, tls_addr);
        let mut cmif_base = tls_addr + self.request.cmif_offset;
        let magic = mmu.read_u32(cmif_base).unwrap_or(0);
        if matches!(self.request.kind, super::hipc::TYPE_CONTROL | super::hipc::TYPE_CONTROL_WITH_CONTEXT) && magic == CMIF_IN_MAGIC {
            return self.handle_control(session_handle, &service, tls_addr, mmu, mmu.read_u32(cmif_base + 8).unwrap_or(0));
        }

        let mut object_handle = session_handle;
        // Domain sessions: a domain header names the target object, and the CMIF payload follows it.
        if self.domains.contains(&session_handle) && self.request.kind != super::hipc::TYPE_CLOSE {
            let dom = super::hipc::read_domain_header(mmu, cmif_base);
            self.domain_reply = true;
            if dom.kind == super::hipc::DOMAIN_CLOSE {
                if dom.object_id != session_handle {
                    self.handles.remove(&dom.object_id);
                }
                self.write_cmif_response(tls_addr, mmu, 0, &[], None);
                return 0;
            }
            service = match self.handles.get(&dom.object_id) {
                Some(s) => s.clone(),
                None => {
                    self.write_cmif_response(tls_addr, mmu, 0xE401, &[], None);
                    return 0;
                }
            };
            object_handle = dom.object_id;
            cmif_base += 16;
        }
        let magic = mmu.read_u32(cmif_base).unwrap_or(0);
        if magic != CMIF_IN_MAGIC {
            // Write generic success even if raw HIPC control
            self.write_cmif_response(tls_addr, mmu, 0, &[], None);
            return 0;
        }

        let cmd_id = mmu.read_u32(cmif_base + 8).unwrap_or(0);
        let args_base = cmif_base + 0x10;

        // libnx-shaped nvdrv requests carry their data in buffers; the harness's inline form keeps the
        // older handler below.
        let r = &self.request;
        let has_buffers = !(r.send_statics.is_empty() && r.send_buffers.is_empty() && r.recv_buffers.is_empty());
        if service == ServiceType::Nvdrv && (has_buffers || matches!(cmd_id, 2 | 4 | 8 | 11 | 12)) {
            return self.handle_nvdrv(cmd_id, args_base, tls_addr, mmu, nvgpu);
        }

        if let Some(res) = self.handle_system_service(&service, cmd_id, args_base, tls_addr, mmu) {
            return res;
        }

        match service {
            ServiceType::Sm => match cmd_id {
                // Command 0: RegisterClient (sends the client's PID in the special header)
                0 => {
                    self.registered_pid = self.request.pid;
                    self.write_cmif_response(tls_addr, mmu, 0, &[], None);
                    0
                }
                // Command 1: GetService(ServiceName u64)
                1 => {
                    let mut name_bytes = [0u8; 8];
                    let _ = mmu.read_bytes(args_base, &mut name_bytes);
                    let len = name_bytes.iter().position(|&b| b == 0).unwrap_or(8);
                    let srv_name = String::from_utf8_lossy(&name_bytes[..len]).to_string();

                    let target_service = match srv_name.as_str() {
                        "nvdrv" | "nvdrv:a" | "nvdrv:s" | "nvdrv:t" => ServiceType::Nvdrv,
                        "vi:u" | "vi:m" | "vi:s" => ServiceType::Vi,
                        "hid" => ServiceType::Hid,
                        "fsp-srv" => ServiceType::FspSrv,
                        "audout:u" => ServiceType::AudOut,
                        "audren:u" => ServiceType::AudRen,
                        "appletOE" => ServiceType::AppletOE,
                        "appletAE" => ServiceType::AppletAE,
                        "set" => ServiceType::Set,
                        "set:sys" => ServiceType::SetSys,
                        "psm" => ServiceType::Psm,
                        "apm" | "apm:p" | "apm:am" => ServiceType::Apm,
                        "ro:1" | "ro:dmnt" | "ldr:ro" => ServiceType::Ro,
                        "pl:u" | "pl:s" => ServiceType::Pl,
                        "time:u" | "time:s" | "time:a" | "time:r" | "time:su" => ServiceType::Time,
                        "prepo:u" | "prepo:a" | "prepo:m" => ServiceType::Prepo,
                        "bcat:u" | "bcat:a" | "bcat:m" => ServiceType::Bcat,
                        other => ServiceType::Generic(other.to_string()),
                    };

                    let srv_handle = self.allocate_handle(target_service);
                    self.write_cmif_response(tls_addr, mmu, 0, &[srv_handle], Some(srv_handle));
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[], None);
                    0
                }
            },

            ServiceType::Nvdrv => match cmd_id {
                // Command 0: Open -> returns (fd, error = 0)
                0 => {
                    let mut path_bytes = [0u8; 64];
                    let _ = mmu.read_bytes(args_base, &mut path_bytes);
                    let len = path_bytes.iter().position(|&b| b == 0).unwrap_or(64);
                    let dev_name = String::from_utf8_lossy(&path_bytes[..len]);
                    let fd = if dev_name.contains("as-gpu") {
                        2
                    } else if dev_name.contains("ctrl-gpu") {
                        3
                    } else {
                        1
                    };
                    self.write_cmif_response(tls_addr, mmu, 0, &[fd, 0], None);
                    0
                }
                // Command 1: Ioctl(fd, ioctl_cmd, buf_addr, word_count)
                1 => {
                    let _fd = mmu.read_u32(args_base).unwrap_or(0);
                    let ioctl_cmd = mmu.read_u32(args_base + 4).unwrap_or(0);
                    let pb_addr = mmu.read_u64(args_base + 8).unwrap_or(0);
                    let word_count = mmu.read_u32(args_base + 16).unwrap_or(0) as usize;

                    if ioctl_cmd == NVGPU_IOCTL_CHANNEL_SUBMIT_GPFIFO && pb_addr != 0 && word_count > 0 {
                        nvgpu.process_pushbuffer(mmu, pb_addr, word_count);
                    } else if ioctl_cmd == 0xC018_4E01 && pb_addr != 0 && word_count > 0 {
                        let mut nvn = crate::gpu::nvn::NvnDriver::new();
                        nvn.submit_guest_command_stream(mmu, nvgpu, pb_addr, word_count);
                    } else if ioctl_cmd == 0x80B0_4705 && pb_addr != 0 {
                        // NVGPU_GPU_IOCTL_GET_CHARACTERISTICS: Write NVIDIA T239 Ampere GA10B (SM 8.7) characteristics
                        let _ = mmu.write_u32(pb_addr, 0x0170);      // arch = NVGPU_GPU_ARCH_GA100 (Ampere)
                        let _ = mmu.write_u32(pb_addr + 4, 0x000B);  // impl = NVGPU_GPU_IMPL_GA10B (Tegra T239)
                        let _ = mmu.write_u32(pb_addr + 8, 0x00A1);  // rev = A1
                        let _ = mmu.write_u32(pb_addr + 12, 12);     // num_gpc / 12 SMs (1536 CUDA cores)
                        let _ = mmu.write_u32(pb_addr + 16, 0x0807); // sm_arch_sm_version = SM 8.7
                    } else if ioctl_cmd == 0xC018_4102 && pb_addr != 0 {
                        // NVGPU_AS_IOCTL_ALLOC_SPACE: Return allocated GPU VA
                        let _ = mmu.write_u64(pb_addr, 0x0000_0000_9800_0000);
                    } else if ioctl_cmd == 0xC018_4714 && pb_addr != 0 {
                        // NVGPU_GPU_IOCTL_GET_L2_CACHE_SIZE: 4MB L2 Cache for Tegra T239
                        let _ = mmu.write_u32(pb_addr, 0x0040_0000);
                    }

                    // Return nvdrv status = 0 (NVOS_STATUS_SUCCESS)
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 3: Initialize
                3 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::AudOut => match cmd_id {
                // Command 0/1: OpenAudioOut / StartAudioOut -> returns (sample_rate=48000, channels=2)
                0 | 1 => {
                    audio.is_playing = true;
                    self.write_cmif_response(
                        tls_addr,
                        mmu,
                        0,
                        &[audio.sample_rate, audio.channels],
                        None,
                    );
                    0
                }
                // Command 3: AppendAudioOutBuffer(pcm_addr, sample_count)
                3 => {
                    let pcm_addr = mmu.read_u64(args_base).unwrap_or(0);
                    let sample_count = mmu.read_u32(args_base + 8).unwrap_or(0) as usize;
                    audio.append_pcm_buffer(mmu, pcm_addr, sample_count);
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::AudRen => match cmd_id {
                // Command 0: OpenAudioRenderer(AudioRendererParameter, aruid; transfer memory, process)
                // -> IAudioRenderer. The parameter block starts with sample rate, sample count, mix buffer
                // count, submix count, voice count and sink count.
                0 => {
                    let p = |i: u64| mmu.read_u32(args_base + i * 4).unwrap_or(0);
                    self.audio_renderer = Some(super::audren::AudioRendererModel::new(super::audren::RendererParams {
                        sample_rate: p(0),
                        sample_count: p(1),
                        mix_buffer_count: p(2),
                        voice_count: p(4),
                        sink_count: p(5),
                    }));
                    let renderer_h = self.allocate_handle(ServiceType::AudioRenderer);
                    self.write_cmif_response(tls_addr, mmu, 0, &[renderer_h], Some(renderer_h));
                    0
                }
                // Command 1: GetWorkBufferSize(AudioRendererParameter) -> u64 (256 KB)
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x40000, 0], None);
                    0
                }
                // Command 4: RequestUpdateAudioRenderer -> returns 0 (Success)
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::AppletOE => match cmd_id {
                // Command 0: OpenApplicationProxy -> returns new ApplicationProxy sub-session handle
                0 => {
                    let proxy_h = self.allocate_handle(ServiceType::ApplicationProxy);
                    self.write_cmif_response(tls_addr, mmu, 0, &[proxy_h], Some(proxy_h));
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
            },

            ServiceType::ApplicationProxy => match cmd_id {
                // Command 0: GetCommonStateGetter -> returns ICommonStateGetter
                0 => {
                    let h = self.allocate_handle(ServiceType::CommonStateGetter);
                    self.write_cmif_response(tls_addr, mmu, 0, &[h], Some(h));
                    0
                }
                // Command 1: GetSelfController -> returns ISelfController
                1 => {
                    let h = self.allocate_handle(ServiceType::SelfController);
                    self.write_cmif_response(tls_addr, mmu, 0, &[h], Some(h));
                    0
                }
                // Command 4: GetDisplayVersion -> returns [23, 0, 1]
                4 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[23, 0, 1], None);
                    0
                }
                // Command 20: GetApplicationFunctions -> returns IApplicationFunctions
                20 => {
                    let h = self.allocate_handle(ServiceType::ApplicationFunctions);
                    self.write_cmif_response(tls_addr, mmu, 0, &[h], Some(h));
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::CommonStateGetter => match cmd_id {
                // Command 1 / 5: GetCurrentFocusState -> returns 1 (InFocus)
                1 | 5 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::SelfController => match cmd_id {
                // Command 10: CreateManagedDisplaySeparableLayer -> returns LayerId = 1
                10 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1, 0], None);
                    0
                }
                // Command 16: AcquireForegroundRights -> returns ResultSuccess (0)
                16 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 40: SetPerformanceConfiguration(u32 mode, u32 config)
                40 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::ApplicationFunctions => match cmd_id {
                // Command 1: SetTerminateResult
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 20: EnsureSaveData
                20 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 21: GetDesiredLanguage -> returns 0 (en-US / American English)
                21 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 40: NotifyRunning
                40 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Vi => match cmd_id {
                // Command 2: GetDisplayService -> returns IApplicationDisplayService
                2 => {
                    let disp_h = self.allocate_handle(ServiceType::ApplicationDisplayService);
                    self.write_cmif_response(tls_addr, mmu, 0, &[disp_h], Some(disp_h));
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::ViSystemDisplay | ServiceType::ViManagerDisplay => {
                self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
                0
            }
            ServiceType::ViBinderRelay => match cmd_id {
                // Binder relay: adjust strong/weak references.
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[], None);
                    0
                }
                // Binder relay: return a copyable event handle. Its level follows whether the
                // modeled producer currently has a configured buffer available for dequeue.
                2 => {
                    let event = self.allocate_handle(ServiceType::ServiceEvent("vi-binder"));
                    self.reply(tls_addr, mmu, 0, &[], &[event], None);
                    0
                }
                // Binder relay: transact with the modeled IGraphicBufferProducer queue.
                3 => self.handle_vi_binder_transact(tls_addr, mmu, args_base),
                _ => {
                    self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
                    0
                }
            },
            ServiceType::ApplicationDisplayService => match cmd_id {
                100 | 101 | 102 | 103 => {
                    let service = match cmd_id {
                        100 | 103 => ServiceType::ViBinderRelay,
                        101 => ServiceType::ViSystemDisplay,
                        _ => ServiceType::ViManagerDisplay,
                    };
                    self.reply_object(tls_addr, mmu, service)
                }
                // Command 1010: OpenDisplay("Default") -> returns DisplayId = 0
                1010 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0, 0], None);
                    0
                }
                // Command 2020: OpenLayer. libnx expects an Android native-window parcel in
                // the B receive buffer, plus its byte length as the CMIF output value.
                2020 => self.handle_vi_open_layer(tls_addr, mmu, None),
                // Command 2030: CreateStrayLayer -> returns LayerId = 1, IGraphicBufferProducer = 0x100
                2030 => {
                    self.handle_vi_open_layer(tls_addr, mmu, Some(1))
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Hid => match cmd_id {
                // Command 0: CreateAppletResource -> returns IAppletResource
                0 => {
                    let res_h = self.allocate_handle(ServiceType::AppletResource);
                    self.write_cmif_response(tls_addr, mmu, 0, &[res_h], Some(res_h));
                    0
                }
                _ if self.is_libnx_request() => self.handle_hid(cmd_id, args_base, tls_addr, mmu),
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::AppletResource => match cmd_id {
                // Command 0: GetSharedMemoryHandle -> copy handle to the HID shared memory object
                0 => {
                    let shm = self.allocate_handle(ServiceType::SharedMemory { size: HID_SHARED_MEMORY_SIZE });
                    self.hid.handle = Some(shm);
                    self.reply(tls_addr, mmu, 0, &[], &[shm], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::FspSrv => match cmd_id {
                // Command 1: SetCurrentProcess(u64, PID) -> no output
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[], None);
                    0
                }
                // Command 0 / 200: OpenDataStorageByCurrentProcess -> returns IStorage
                0 | 200 => {
                    let storage_h = self.allocate_handle(ServiceType::Storage);
                    self.write_cmif_response(tls_addr, mmu, 0, &[storage_h], Some(storage_h));
                    0
                }
                // Command 18: OpenSdCardFileSystem -> returns IFileSystem ("sdmc:/")
                18 => {
                    let fs_h = self.allocate_handle(ServiceType::FileSystem("sdmc:/".to_string()));
                    self.write_cmif_response(tls_addr, mmu, 0, &[fs_h], Some(fs_h));
                    0
                }
                // Command 51 | 52 | 53: OpenSaveDataFileSystem -> returns IFileSystem ("save:/")
                51 | 52 | 53 => {
                    let fs_h = self.allocate_handle(ServiceType::FileSystem("save:/".to_string()));
                    self.write_cmif_response(tls_addr, mmu, 0, &[fs_h], Some(fs_h));
                    0
                }
                // Command 10: OpenDirectory -> returns IDirectory
                10 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let target_path = if path.is_empty() { "sdmc:/".to_string() } else { path };
                    let entries = vfs.list_directory(&target_path).unwrap_or_default();
                    let dir_h = self.allocate_handle(ServiceType::Directory {
                        path: target_path,
                        entries,
                        index: 0,
                    });
                    self.write_cmif_response(tls_addr, mmu, 0, &[dir_h], Some(dir_h));
                    0
                }
                // Command 11: ReadDirectory
                11 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let target_path = if path.is_empty() { "sdmc:/".to_string() } else { path };
                    let entries = vfs.list_directory(&target_path).unwrap_or_default();
                    let count = entries.len() as u64;
                    let lo = (count & 0xFFFF_FFFF) as u32;
                    let hi = ((count >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 12: GetFreeSpaceSize -> returns u64 free bytes
                12 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let target_path = if path.is_empty() { "sdmc:/".to_string() } else { path };
                    let free_bytes = vfs.get_free_space_size(&target_path);
                    let lo = (free_bytes & 0xFFFF_FFFF) as u32;
                    let hi = ((free_bytes >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::FileSystem(mount) => self.handle_filesystem(&mount, cmd_id, args_base, tls_addr, mmu, vfs),
            ServiceType::Directory { path, entries, index } => {
                self.handle_directory(object_handle, path, entries, index, cmd_id, args_base, tls_addr, mmu, vfs)
            }
            ServiceType::File(handle) => self.handle_file(handle, cmd_id, args_base, tls_addr, mmu, vfs),

            ServiceType::Storage => match cmd_id {
                // Command 4: GetSize -> returns 0x40000 (256 KB)
                4 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x40000, 0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::SetSys => match cmd_id {
                // Command 0 / 1: GetAvailableLanguageCodes -> returns count=1, code=0 (en-US = 0)
                0 | 1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1, 0], None);
                    0
                }
                // Command 2: GetRegionCode -> returns 1 (USA / NorthAmerica)
                2 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
                // Command 3: GetSystemLanguage -> returns 0 (en-US = 0)
                3 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 4: GetFirmwareVersion -> returns the target system version 23.0.1
                4 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[23, 0, 1], None);
                    0
                }
                // Command 5: GetColorSetId -> returns 1 (BasicBlack)
                5 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
                // Command 6 | 77: GetDeviceNickName -> "Ounce-T239"
                6 | 77 => {
                    let nickname = b"Ounce-T239\0";
                    let buf_addr = mmu.read_u64(args_base).unwrap_or(0);
                    if buf_addr >= 0x10000 && mmu.is_mapped(buf_addr) {
                        let _ = mmu.write_bytes(buf_addr, nickname);
                    }
                    let mut words = [0u32; 8];
                    for (i, chunk) in nickname.chunks(4).enumerate() {
                        if i < 8 {
                            let mut b = [0u8; 4];
                            b[..chunk.len()].copy_from_slice(chunk);
                            words[i] = u32::from_le_bytes(b);
                        }
                    }
                    self.write_cmif_response(tls_addr, mmu, 0, &words, None);
                    0
                }
                // Command 8 | 67: GetProductModel / GetConsoleModel -> returns 0x02 (Drake / T239)
                8 | 67 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x02], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[23, 0, 1], None);
                    0
                }
            },

            ServiceType::Ro => match cmd_id {
                // Command 0: LoadNro(u64 expected_process_id, u64 nro_addr, u64 nro_size, u64 bss_addr, u64 bss_size) -> (u64 nro_loaded_base)
                0 => {
                    let _pid = mmu.read_u64(args_base).unwrap_or(0);
                    let nro_addr = mmu.read_u64(args_base + 8).unwrap_or(0);
                    let nro_size = mmu.read_u64(args_base + 16).unwrap_or(0);
                    let _bss_addr = mmu.read_u64(args_base + 24).unwrap_or(0);
                    let _bss_size = mmu.read_u64(args_base + 32).unwrap_or(0);

                    if nro_addr != 0 && nro_size >= 0x80 {
                        let mut magic = [0u8; 4];
                        if mmu.read_bytes(nro_addr + 0x10, &mut magic).is_ok() && &magic == b"NRO0" {
                            let mod0_offset = mmu.read_u32(nro_addr + 4).unwrap_or(0) as u64;
                            if mod0_offset > 0 {
                                let mut mod0_magic = [0u8; 4];
                                if mmu.read_bytes(nro_addr + mod0_offset, &mut mod0_magic).is_ok() && &mod0_magic == b"MOD0" {
                                    // Valid NRO0 / MOD0 identified
                                }
                            }
                        }
                    }

                    let lo = (nro_addr & 0xFFFF_FFFF) as u32;
                    let hi = ((nro_addr >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 1: UnloadNro(u64 nro_loaded_base)
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 2: LoadNrr(u64 expected_process_id, u64 nrr_addr, u64 nrr_size)
                2 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 3: UnloadNrr(u64 nrr_addr)
                3 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Pl => match cmd_id {
                // Command 0: RequestSharedFontLoad
                0 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 1: GetSharedFontAddress
                1 => {
                    // Ensure Shared Font Memory is mapped at SHARED_FONT_ADDR
                    if !mmu.is_mapped(SHARED_FONT_ADDR) {
                        mmu.map_range(SHARED_FONT_ADDR, SHARED_FONT_SIZE, crate::mmu::Permission::READ_WRITE);
                        let font_hdr = b"OUNCEFNT\x00\x01\x00\x00"; // placeholder header only; no font glyph data is bundled
                        let _ = mmu.write_bytes(SHARED_FONT_ADDR, font_hdr);
                    }
                    let lo = (SHARED_FONT_ADDR & 0xFFFF_FFFF) as u32;
                    let hi = ((SHARED_FONT_ADDR >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 2: GetSharedFontSize
                2 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[SHARED_FONT_SIZE as u32, 0], None);
                    0
                }
                // Command 3: GetSharedFontSharedMemoryHandle
                3 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x300], Some(0x300));
                    0
                }
                // Command 4: GetSharedFontType -> returns 0 (placeholder font format)
                4 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 5: GetSharedFontGlyphMetrics -> returns [glyph_count=256, cell_w=16, cell_h=16, baseline=12]
                5 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[256, 16, 16, 12], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Time => match cmd_id {
                // Command 0: GetStandardUserSystemClock -> ISystemClock
                0 => {
                    let clock_h = self.allocate_handle(ServiceType::SystemClock);
                    self.write_cmif_response(tls_addr, mmu, 0, &[clock_h], Some(clock_h));
                    0
                }
                // Command 1: GetStandardNetworkSystemClock -> ISystemClock
                1 => {
                    let clock_h = self.allocate_handle(ServiceType::SystemClock);
                    self.write_cmif_response(tls_addr, mmu, 0, &[clock_h], Some(clock_h));
                    0
                }
                // Command 2: GetStandardSteadyClock -> ISteadyClock
                2 => {
                    let clock_h = self.allocate_handle(ServiceType::SteadyClock);
                    self.write_cmif_response(tls_addr, mmu, 0, &[clock_h], Some(clock_h));
                    0
                }
                // Command 3: GetTimeZoneService
                3 => {
                    let tz_h = self.allocate_handle(ServiceType::Generic("ITimeZoneService".to_string()));
                    self.write_cmif_response(tls_addr, mmu, 0, &[tz_h], Some(tz_h));
                    0
                }
                // Command 4: GetStandardLocalSystemClock / GetStandardNetworkClock -> ISystemClock
                4 => {
                    let clock_h = self.allocate_handle(ServiceType::SystemClock);
                    self.write_cmif_response(tls_addr, mmu, 0, &[clock_h], Some(clock_h));
                    0
                }
                // Command 20 | 100: GetCurrentSteadyClockTime / Monotonic Clock (matching host Instant with nanosecond precision)
                20 | 100 => {
                    let nanos = self.start_instant.elapsed().as_nanos() as u64;
                    let lo = (nanos & 0xFFFF_FFFF) as u32;
                    let hi = ((nanos >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 21: GetPosixTimeWithAutomaticCorrection / POSIX Time Clock (standard Horizon OS UTC epoch timestamp)
                21 => {
                    let now_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(1793000000);
                    let lo = (now_secs & 0xFFFF_FFFF) as u32;
                    let hi = ((now_secs >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::SystemClock => match cmd_id {
                // Command 0: GetCurrentTime -> returns 64-bit UTC epoch timestamp
                0 => {
                    let now_secs = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|d| d.as_secs())
                        .unwrap_or(1793000000);
                    let lo = (now_secs & 0xFFFF_FFFF) as u32;
                    let hi = ((now_secs >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::SteadyClock => match cmd_id {
                // Command 0 | 1: GetCurrentTimePoint -> returns 64-bit monotonic clock in nanoseconds
                0 | 1 => {
                    let nanos = self.start_instant.elapsed().as_nanos() as u64;
                    let lo = (nanos & 0xFFFF_FFFF) as u32;
                    let hi = ((nanos >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Prepo => {
                // Command 0 / 1 / 2: SaveReport / PlayReport (Telemetry stubs)
                self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                0
            }

            ServiceType::Bcat => {
                // Command 0 / 1: RequestDeliveryCache (BCAT content delivery stubs)
                self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                0
            }

            ServiceType::Generic(name) => {
                // A service this model does not implement: report it rather than pretend the call worked.
                eprintln!("[HIPC] '{name}' command {cmd_id} not implemented");
                self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
                0
            }

            // Not a session: requests on it are invalid.
            ServiceType::SharedMemory { .. }
            | ServiceType::TransferMemory { .. }
            | ServiceType::NvEvent { .. }
            | ServiceType::AudioRendererEvent => 0xE401,

            ServiceType::AudioRenderer => self.handle_audio_renderer(cmd_id, tls_addr, mmu),

            // Routed by `handle_system_service` above.
            ServiceType::AppletAE
            | ServiceType::Applet(_)
            | ServiceType::Set
            | ServiceType::TimeZoneService
            | ServiceType::Psm
            | ServiceType::PsmSession
            | ServiceType::Apm
            | ServiceType::ApmSession
            | ServiceType::ServiceEvent(_) => 0xE401,
        }
    }

    /// Supplies the native-window parcel consumed by libnx's `viCreateLayer`, identifying the
    /// modeled graphic-buffer producer endpoint used by the relay transactions below.
    fn handle_vi_open_layer(&self, tls_addr: u64, mmu: &mut Memory, layer_id: Option<u64>) -> u64 {
        const PARCEL_SIZE: u64 = 0x1c;
        let Some(buffer) = self.request.recv_buffers.iter().find(|b| b.size > 0).or_else(|| self.request.recv_list.iter().find(|b| b.size > 0)) else {
            self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
            return 0;
        };
        if buffer.size < PARCEL_SIZE {
            self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
            return 0;
        }

        let mut parcel = [0u8; PARCEL_SIZE as usize];
        // ParcelHeader: 12-byte payload at offset 16, no flattened objects.
        parcel[0..4].copy_from_slice(&12u32.to_le_bytes());
        parcel[4..8].copy_from_slice(&16u32.to_le_bytes());
        parcel[8..12].copy_from_slice(&0u32.to_le_bytes());
        parcel[12..16].copy_from_slice(&0u32.to_le_bytes());
        // NativeWindow parcel's third payload word is the IGraphicBufferProducer binder ID.
        parcel[24..28].copy_from_slice(&0x100u32.to_le_bytes());
        let written = self.write_out_buffer(mmu, &parcel);
        if written != parcel.len() {
            self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
            return 0;
        }

        let mut out = Vec::with_capacity(16);
        if let Some(id) = layer_id {
            out.extend_from_slice(&id.to_le_bytes());
        }
        out.extend_from_slice(&PARCEL_SIZE.to_le_bytes());
        let words: Vec<u32> = out.chunks_exact(4).map(|w| u32::from_le_bytes(w.try_into().unwrap())).collect();
        self.write_cmif_response(tls_addr, mmu, 0, &words, None);
        0
    }

    fn handle_vi_binder_transact(&mut self, tls_addr: u64, mmu: &mut Memory, args_base: u64) -> u64 {
        let code = mmu.read_u32(args_base + 4).unwrap_or(0);
        let Some(out) = self.request.recv_buffers.iter().find(|b| b.size > 0).or_else(|| self.request.recv_list.iter().find(|b| b.size > 0)) else {
            self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
            return 0;
        };

        let input = self.request.send_buffers.iter().find(|b| b.size > 0).copied()
            .or_else(|| self.request.send_statics.iter().map(|(_, b)| *b).find(|b| b.size > 0));
        let args = input.and_then(|b| read_binder_args(mmu, b.addr, b.size)).unwrap_or_default();
        let arg_i32 = |off: usize| args.get(off..off + 4)
            .map(|v| i32::from_le_bytes(v.try_into().unwrap()));

        let mut payload = Vec::new();
        let mut status = 0i32;
        match code {
            // REQUEST_BUFFER: the producer already owns the preallocated metadata.
            1 => {
                let slot = arg_i32(0).unwrap_or(-1);
                if !(0..64).contains(&slot) || self.vi_buffer_queue.configured & (1u64 << slot) == 0 {
                    status = -22;
                } else {
                    payload.extend_from_slice(&0u32.to_le_bytes());
                }
            }
            2 => {} // SET_BUFFER_COUNT
            // DEQUEUE_BUFFER returns one configured slot and no fence object.
            3 => {
                if !self.vi_buffer_queue.connected || self.vi_buffer_queue.dequeued.is_some() {
                    status = -11;
                } else if let Some(slot) = self.vi_buffer_queue.next_configured_slot() {
                    self.vi_buffer_queue.dequeued = Some(slot);
                    payload.extend_from_slice(&(slot as i32).to_le_bytes());
                    payload.extend_from_slice(&0u32.to_le_bytes());
                } else {
                    status = -11;
                }
            }
            4 => { // DETACH_BUFFER
                if let Some(slot) = arg_i32(0).filter(|s| (0..64).contains(s)) {
                    self.vi_buffer_queue.configured &= !(1u64 << slot);
                    if self.vi_buffer_queue.dequeued == Some(slot as u8) {
                        self.vi_buffer_queue.dequeued = None;
                    }
                } else {
                    status = -22;
                }
            }
            7 => { // QUEUE_BUFFER
                let slot = arg_i32(0).unwrap_or(-1);
                let valid_slot = u8::try_from(slot).ok().filter(|slot| *slot < 64);
                if valid_slot.is_none() || self.vi_buffer_queue.dequeued != valid_slot {
                    status = -22;
                } else {
                    self.vi_buffer_queue.dequeued = None;
                    append_vi_buffer_output(&mut payload, self.vi_buffer_queue.configured.count_ones());
                }
            }
            8 => { // CANCEL_BUFFER
                let slot = arg_i32(0).unwrap_or(-1);
                let valid_slot = u8::try_from(slot).ok().filter(|slot| *slot < 64);
                if valid_slot.is_none() || self.vi_buffer_queue.dequeued != valid_slot {
                    status = -22;
                } else {
                    self.vi_buffer_queue.dequeued = None;
                }
            }
            9 => payload.extend_from_slice(&0i32.to_le_bytes()), // QUERY
            10 => { // CONNECT
                self.vi_buffer_queue.connected = true;
                append_vi_buffer_output(&mut payload, 0);
            }
            11 => { // DISCONNECT
                self.vi_buffer_queue.connected = false;
                self.vi_buffer_queue.dequeued = None;
            }
            14 => { // SET_PREALLOCATED_BUFFER
                let slot = arg_i32(0).unwrap_or(-1);
                let present = arg_i32(4).unwrap_or(0) != 0;
                if !(0..64).contains(&slot) {
                    status = -22;
                } else if present {
                    self.vi_buffer_queue.configured |= 1u64 << slot;
                } else {
                    self.vi_buffer_queue.configured &= !(1u64 << slot);
                    if self.vi_buffer_queue.dequeued == Some(slot as u8) {
                        self.vi_buffer_queue.dequeued = None;
                    }
                }
            }
            _ => status = -74,
        }
        if !matches!(code, 8 | 14) {
            if status != 0 {
                payload.clear();
                match code {
                    1 | 9 => payload.extend_from_slice(&0u32.to_le_bytes()),
                    3 => payload.extend_from_slice(&[0; 8]),
                    7 | 10 => append_vi_buffer_output(&mut payload, 0),
                    _ => {}
                }
            }
            payload.extend_from_slice(&status.to_le_bytes());
        }
        let mut parcel = Vec::with_capacity(16 + payload.len());
        parcel.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        parcel.extend_from_slice(&16u32.to_le_bytes());
        parcel.extend_from_slice(&0u32.to_le_bytes());
        parcel.extend_from_slice(&0u32.to_le_bytes());
        parcel.extend_from_slice(&payload);
        if out.size < parcel.len() as u64 || self.write_out_buffer(mmu, &parcel) != parcel.len() {
            self.write_cmif_response(tls_addr, mmu, RESULT_UNKNOWN_COMMAND, &[], None);
        } else {
            self.write_cmif_response(tls_addr, mmu, 0, &[], None);
        }
        0
    }

    /// `IAudioRenderer` commands (libnx `audren`).
    fn handle_audio_renderer(&mut self, cmd_id: u32, tls_addr: u64, mmu: &mut Memory) -> u64 {
        let Some(r) = self.audio_renderer.as_ref() else {
            self.write_cmif_response(tls_addr, mmu, 0xE401, &[], None);
            return 0;
        };
        let (params, started) = (r.params, r.started);
        match cmd_id {
            // GetSampleRate / GetSampleCount / GetMixBufferCount -> u32
            0 => self.write_cmif_response(tls_addr, mmu, 0, &[params.sample_rate], None),
            1 => self.write_cmif_response(tls_addr, mmu, 0, &[params.sample_count], None),
            2 => self.write_cmif_response(tls_addr, mmu, 0, &[params.mix_buffer_count], None),
            // GetState -> u32 (0 started, 1 stopped)
            3 => {
                let state = if started { 0 } else { 1 };
                self.write_cmif_response(tls_addr, mmu, 0, &[state], None);
            }
            // RequestUpdate(in update data; out output data, out performance data), buffers A/B or X/C
            4 | 10 => {
                let req = self.request.clone();
                let input = req.send_buffers.first().copied().filter(|b| b.size > 0).or(req.send_statics.first().map(|&(_, b)| b)).unwrap_or_default();
                let output = req.recv_buffers.first().copied().filter(|b| b.size > 0).or(req.recv_list.first().copied()).unwrap_or_default();
                let mut inb = vec![0u8; input.size.min(0x10_0000) as usize];
                let _ = mmu.read_bytes(input.addr, &mut inb);
                let mut outb = vec![0u8; output.size.min(0x10_0000) as usize];
                let result = self.audio_renderer.as_mut().unwrap().update(&inb, &mut outb);
                let _ = mmu.write_bytes(output.addr, &outb);
                self.write_cmif_response(tls_addr, mmu, result, &[], None);
            }
            // Start / Stop
            5 | 6 => {
                self.audio_renderer.as_mut().unwrap().started = cmd_id == 5;
                self.write_cmif_response(tls_addr, mmu, 0, &[], None);
            }
            // QuerySystemEvent -> copy handle, signaled once per rendered frame
            7 => {
                let h = self.allocate_handle(ServiceType::AudioRendererEvent);
                self.reply(tls_addr, mmu, 0, &[], &[h], None);
            }
            // GetRenderingTimeLimit -> u32 percent; SetRenderingTimeLimit and the rest accepted
            9 => self.write_cmif_response(tls_addr, mmu, 0, &[100], None),
            _ => self.write_cmif_response(tls_addr, mmu, 0, &[], None),
        }
        0
    }

    /// Whether `handle` is the audio renderer's system event.
    pub fn is_audio_renderer_event(&self, handle: u32) -> bool {
        self.handles.get(&handle) == Some(&ServiceType::AudioRendererEvent)
    }

    /// `nvdrv` commands in the form libnx sends them. Each reply carries the nvdrv status word that
    /// libnx turns into a result with `nvConvertError`.
    fn handle_nvdrv(&mut self, cmd_id: u32, args_base: u64, tls_addr: u64, mmu: &mut Memory, nvgpu: &mut NvgpuChannel) -> u64 {
        let req = self.request.clone();
        // The i-th input is an X buffer with a size, else the i-th A buffer (libnx "auto select");
        // outputs likewise use C, else B.
        let input = |i: usize| {
            req.send_statics.get(i).map(|&(_, b)| b).filter(|b| b.size > 0).or(req.send_buffers.get(i).copied())
        };
        let output = |i: usize| req.recv_list.get(i).copied().filter(|b| b.size > 0).or(req.recv_buffers.get(i).copied());
        let read_buf = |mmu: &Memory, b: Option<super::hipc::Buffer>, len: usize| {
            let mut v = vec![0u8; len];
            if let Some(b) = b {
                let n = len.min(b.size as usize);
                let _ = mmu.read_bytes(b.addr, &mut v[..n]);
            }
            v
        };
        match cmd_id {
            // Open(path in A) -> (fd, status)
            0 => {
                let buf = input(0).unwrap_or_default();
                let path = read_buf(mmu, Some(buf), (buf.size as usize).min(0x100));
                let len = path.iter().position(|&b| b == 0).unwrap_or(path.len());
                let (fd, status) = self.nvdrv.open(&String::from_utf8_lossy(&path[..len]));
                self.write_cmif_response(tls_addr, mmu, 0, &[fd, status], None);
            }
            // Ioctl(fd, request), argument block in X/A, written back to C/B -> status.
            // Ioctl2 (11) adds an input buffer; Ioctl3 (12) adds an output buffer.
            1 | 11 | 12 => {
                use super::nvdrv::{ioctl_has_input, ioctl_has_output, ioctl_size, IoctlExtra};
                let fd = mmu.read_u32(args_base).unwrap_or(0);
                let request = mmu.read_u32(args_base + 4).unwrap_or(0);
                let size = ioctl_size(request);
                let mut data = if ioctl_has_input(request) { read_buf(mmu, input(0), size) } else { vec![0u8; size] };
                let inbuf = if cmd_id == 11 {
                    let b = input(1).unwrap_or_default();
                    read_buf(mmu, Some(b), b.size as usize)
                } else {
                    Vec::new()
                };
                let out2 = if cmd_id == 12 { output(1) } else { None };
                let mut outbuf = vec![0u8; out2.map_or(0, |b| b.size as usize)];
                let extra = IoctlExtra { inbuf: &inbuf, outbuf: out2.map(|_| outbuf.as_mut_slice()) };
                // Blocked 3D channels may be free now (the guest can release a semaphore with a
                // plain store), so syncpoint reads and waits below see current fences.
                self.nvdrv.poll_3d(mmu);
                let status = self.nvdrv.ioctl_ex(fd, request, &mut data, extra);
                if ioctl_has_output(request) {
                    if let Some(b) = output(0) {
                        let n = data.len().min(b.size as usize);
                        let _ = mmu.write_bytes(b.addr, &data[..n]);
                    }
                }
                if let Some(b) = out2 {
                    let _ = mmu.write_bytes(b.addr, &outbuf);
                }
                // Submitted pushbuffer segments run now: on the channel's 3D engine when it allocated
                // MAXWELL_B or AMPERE_B, else on the older synthetic-method model.
                for (fd, addr, words) in self.nvdrv.take_channel_submissions() {
                    if !self.nvdrv.run_3d(mmu, fd, addr, words) {
                        nvgpu.process_pushbuffer(mmu, addr, words);
                    }
                }
                self.nvdrv.poll_3d(mmu);
                self.write_cmif_response(tls_addr, mmu, 0, &[status], None);
            }
            // Close(fd) -> status
            2 => {
                let status = self.nvdrv.close(mmu.read_u32(args_base).unwrap_or(0));
                self.write_cmif_response(tls_addr, mmu, 0, &[status], None);
            }
            // QueryEvent(fd, event_id) -> status, copy handle to a readable event
            4 => {
                let fd = mmu.read_u32(args_base).unwrap_or(0);
                let event_id = mmu.read_u32(args_base + 4).unwrap_or(0);
                if self.nvdrv.fds.contains_key(&fd) {
                    let h = self.allocate_handle(ServiceType::NvEvent { fd, event_id });
                    self.reply(tls_addr, mmu, 0, &super::nvdrv::NV_OK.to_le_bytes(), &[h], None);
                } else {
                    self.write_cmif_response(tls_addr, mmu, 0, &[super::nvdrv::NV_BAD_PARAMETER], None);
                }
            }
            // Initialize (tmem size, process + transfer memory handles) / SetClientPID -> status
            _ => self.write_cmif_response(tls_addr, mmu, 0, &[super::nvdrv::NV_OK], None),
        }
        0
    }

    /// A path argument: libnx sends `FsPath` in the first send static (X) buffer; older callers put a
    /// string (or a pointer to one) in the CMIF arguments at `fallback`.
    fn path_arg(&self, mmu: &Memory, fallback: u64) -> String {
        self.path_arg_at(mmu, 0, fallback)
    }

    /// Path argument `index`: X buffer `index`, else the CMIF arguments at `fallback`.
    pub(super) fn path_arg_at(&self, mmu: &Memory, index: usize, fallback: u64) -> String {
        if let Some(&(_, buf)) = self.request.send_statics.get(index) {
            let mut bytes = vec![0u8; (buf.size as usize).min(0x301)];
            if mmu.read_bytes(buf.addr, &mut bytes).is_ok() {
                let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                return String::from_utf8_lossy(&bytes[..len]).to_string();
            }
        }
        read_cmif_string(mmu, fallback, 256)
    }

    /// The guest mapped shared memory object `handle` at `addr` (svcMapSharedMemory).
    pub fn shared_memory_mapped(&mut self, handle: u32, addr: u64, mmu: &mut Memory) {
        if self.hid.handle == Some(handle) {
            self.hid.mapped(addr, mmu);
        }
        self.time_shared_memory_mapped(handle, addr, mmu);
    }

    /// The guest unmapped shared memory at `addr` (svcUnmapSharedMemory).
    pub fn shared_memory_unmapped(&mut self, addr: u64) {
        self.hid.unmapped(addr);
    }

    /// HIPC control requests (`TYPE_CONTROL`): domain conversion, cloning, pointer buffer size.
    fn handle_control(&mut self, session_handle: u32, service: &ServiceType, tls_addr: u64, mmu: &mut Memory, cmd_id: u32) -> u64 {
        match cmd_id {
            // CloneCurrentObject / CloneCurrentObjectEx -> a new session to the same service
            2 | 4 => {
                let h = self.allocate_handle(service.clone());
                self.write_cmif_response(tls_addr, mmu, 0, &[], Some(h));
            }
            // QueryPointerBufferSize -> u16 size
            3 => self.write_cmif_response(tls_addr, mmu, 0, &[POINTER_BUFFER_SIZE], None),
            // ConvertCurrentObjectToDomain -> u32 object ID. The session's own handle is its first object.
            0 => {
                self.domains.insert(session_handle);
                self.write_cmif_response(tls_addr, mmu, 0, &[session_handle], None);
            }
            _ => self.write_cmif_response(tls_addr, mmu, 0, &[], None),
        }
        0
    }

    /// Writes a standard CMIF ("SFCO") response frame back into the thread's TLS buffer.
    pub(super) fn write_cmif_response(
        &self,
        tls_addr: u64,
        mmu: &mut Memory,
        result_code: u32,
        out_words: &[u32],
        move_handle: Option<u32>,
    ) {
        let data: Vec<u8> = out_words.iter().flat_map(|w| w.to_le_bytes()).collect();
        self.reply(tls_addr, mmu, result_code, &data, &[], move_handle);
    }

    /// Writes a response. `new_object` is a session this request created: a move handle on a plain session,
    /// an out object ID on a domain. Handlers echo that handle as their first out word for the harness's
    /// fixed-offset reads; on a domain the echo is dropped so the object ID lands right after the real data.
    pub(super) fn reply(&self, tls_addr: u64, mmu: &mut Memory, result: u32, data: &[u8], copy_handles: &[u32], new_object: Option<u32>) {
        let objects: Vec<u32> = new_object.into_iter().collect();
        if self.domain_reply {
            let data = match new_object {
                Some(h) if data.len() >= 4 && data[..4] == h.to_le_bytes() => &data[4..],
                _ => data,
            };
            super::hipc::write_response(
                mmu,
                tls_addr,
                &super::hipc::Response { result, data, copy_handles, domain_objects: Some(&objects), ..Default::default() },
            );
        } else {
            super::hipc::write_response(
                mmu,
                tls_addr,
                &super::hipc::Response { result, data, copy_handles, move_handles: &objects, ..Default::default() },
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::AudioOutServer;
    use crate::fs::VirtualFileSystem;
    use crate::gpu::nvgpu::NvgpuChannel;
    use crate::mmu::Permission;

    #[test]
    fn vi_open_layer_returns_libnx_native_window_parcel() {
        use crate::kernel::cmif::test_util::{Harness, Req, BUF};

        let mut h = Harness::new();
        let display = h.ipc.allocate_handle(ServiceType::ApplicationDisplayService);
        let reply = h.call_out(display, Req { cmd: 2020, pid: true, recv: Some((BUF, 0x100)), ..Default::default() }, 8);
        assert_eq!(reply.result, 0);
        assert_eq!(reply.u64(0), 0x1c);

        let parcel = h.read_buf(BUF, 0x1c);
        let word = |off: usize| u32::from_le_bytes(parcel[off..off + 4].try_into().unwrap());
        assert_eq!((word(0), word(4), word(8), word(12)), (12, 16, 0, 0));
        assert_eq!((word(16), word(20), word(24)), (0, 0, 0x100));
    }

    #[test]
    fn vi_returns_real_binder_relay_session() {
        use crate::kernel::cmif::test_util::{Harness, Req, BUF};

        fn parcel(args: &[u32]) -> Vec<u8> {
            let payload_size = 12 + args.len() * 4;
            let mut out = vec![0u8; 16 + payload_size];
            out[0..4].copy_from_slice(&(payload_size as u32).to_le_bytes());
            out[4..8].copy_from_slice(&16u32.to_le_bytes());
            out[16..20].copy_from_slice(&0x100u32.to_le_bytes());
            for (i, word) in args.iter().enumerate() {
                let off = 28 + i * 4;
                out[off..off + 4].copy_from_slice(&word.to_le_bytes());
            }
            out
        }

        fn transact(h: &mut Harness, relay: u32, code: u32, args: &[u32]) -> Vec<u8> {
            let input_parcel = parcel(args);
            h.mmu.write_bytes(BUF, &input_parcel).unwrap();
            let input = [0x100u32, code, 0].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>();
            let reply = h.call(relay, Req {
                cmd: 3,
                data: &input,
                send: Some((BUF, input_parcel.len() as u64)),
                recv: Some((BUF + 0x1000, 0x400)),
                ..Default::default()
            });
            assert_eq!(reply.result, 0);
            h.read_buf(BUF + 0x1000, 0x400)
        }

        let mut h = Harness::new();
        let display = h.ipc.allocate_handle(ServiceType::ApplicationDisplayService);
        let response = h.call(display, Req { cmd: 100, ..Default::default() });
        assert_eq!(response.result, 0);
        let relay = response.move_handles[0];
        assert_eq!(h.ipc.handles.get(&relay), Some(&ServiceType::ViBinderRelay));


        let adjusted = h.call(relay, Req { cmd: 1, data: &[0x100, 1, 0].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>(), ..Default::default() });
        assert_eq!(adjusted.result, 0);
        let native_handle = h.call(relay, Req { cmd: 2, data: &[0x100, 0x0f].into_iter().flat_map(u32::to_le_bytes).collect::<Vec<_>>(), ..Default::default() });
        assert_eq!(native_handle.result, 0);
        assert_eq!(native_handle.copy_handles.len(), 1);
        assert_eq!(h.ipc.service_event_signaled(native_handle.copy_handles[0]), Some(false));

        let connected = transact(&mut h, relay, 10, &[]);
        assert_eq!(u32::from_le_bytes(connected[0..4].try_into().unwrap()), 20);
        assert_eq!(u32::from_le_bytes(connected[16..20].try_into().unwrap()), 1280);
        assert_eq!(u32::from_le_bytes(connected[20..24].try_into().unwrap()), 720);
        assert_eq!(u32::from_le_bytes(connected[32..36].try_into().unwrap()), 0);

        transact(&mut h, relay, 14, &[0, 1]);
        assert_eq!(h.ipc.service_event_signaled(native_handle.copy_handles[0]), Some(true));
        let dequeued = transact(&mut h, relay, 3, &[]);
        assert_eq!(i32::from_le_bytes(dequeued[16..20].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(dequeued[20..24].try_into().unwrap()), 0);
        assert_eq!(i32::from_le_bytes(dequeued[24..28].try_into().unwrap()), 0);
        assert_eq!(h.ipc.service_event_signaled(native_handle.copy_handles[0]), Some(false));
        let requested = transact(&mut h, relay, 1, &[0]);
        assert_eq!(u32::from_le_bytes(requested[16..20].try_into().unwrap()), 0);
        assert_eq!(i32::from_le_bytes(requested[20..24].try_into().unwrap()), 0);
        let queued = transact(&mut h, relay, 7, &[0]);
        assert_eq!(u32::from_le_bytes(queued[16..20].try_into().unwrap()), 1280);
        assert_eq!(i32::from_le_bytes(queued[32..36].try_into().unwrap()), 0);
        assert_eq!(h.ipc.service_event_signaled(native_handle.copy_handles[0]), Some(true));
        let invalid = transact(&mut h, relay, 7, &[u32::MAX]);
        assert_eq!(i32::from_le_bytes(invalid[32..36].try_into().unwrap()), -22);
        assert_eq!(h.ipc.service_event_signaled(native_handle.copy_handles[0]), Some(true));
    }

    /// libnx-shaped requests: PID in the special header, CMIF data aligned after it, and handles
    /// returned in the right list (move for sessions, copy for shared memory).
    #[test]
    fn libnx_shaped_sm_and_hid_requests() {
        let mut mmu = Memory::new();
        let tls = 0x8000_0000u64;
        mmu.map_range(tls, 4096, Permission::READ_WRITE);
        let mut ipc = IpcManager::new();
        let (mut nvgpu, mut audio, mut vfs) = (NvgpuChannel::new(), AudioOutServer::new(), VirtualFileSystem::new());
        let sm = ipc.allocate_handle(ServiceType::Sm);

        // sm: RegisterClient with PID: header 8 + special 4 + pid 8 = 20 -> CMIF at 0x20.
        let request = |mmu: &mut Memory, with_pid: bool, cmd: u32, arg: u64| {
            for i in 0..0x40 {
                mmu.write_u32(tls + i * 4, 0).unwrap();
            }
            mmu.write_u32(tls, 4).unwrap();
            mmu.write_u32(tls + 4, 10 | (with_pid as u32) << 31).unwrap();
            let cmif = if with_pid {
                mmu.write_u32(tls + 8, 1).unwrap();
                mmu.write_u64(tls + 12, 0x77).unwrap();
                0x20
            } else {
                0x10
            };
            mmu.write_u32(tls + cmif, CMIF_IN_MAGIC).unwrap();
            mmu.write_u32(tls + cmif + 8, cmd).unwrap();
            mmu.write_u64(tls + cmif + 16, arg).unwrap();
        };
        request(&mut mmu, true, 0, 0);
        assert_eq!(ipc.handle_sync_request(sm, tls, &mut mmu, &mut nvgpu, &mut audio, &mut vfs), 0);
        assert_eq!(ipc.registered_pid, Some(0x77));
        assert_eq!(mmu.read_u32(tls + 0x10).unwrap(), CMIF_OUT_MAGIC);

        // GetServiceHandle("hid") -> one move handle.
        request(&mut mmu, false, 1, u64::from_le_bytes(*b"hid\0\0\0\0\0"));
        ipc.handle_sync_request(sm, tls, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(mmu.read_u32(tls + 4).unwrap() >> 31, 1);
        assert_eq!(mmu.read_u32(tls + 8).unwrap(), 1 << 5);
        let hid = mmu.read_u32(tls + 12).unwrap();
        assert_eq!(ipc.handles.get(&hid), Some(&ServiceType::Hid));

        // hid CreateAppletResource (PID) -> IAppletResource; GetSharedMemoryHandle -> one copy handle.
        request(&mut mmu, true, 0, 0);
        ipc.handle_sync_request(hid, tls, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        let res = mmu.read_u32(tls + 12).unwrap();
        assert_eq!(ipc.handles.get(&res), Some(&ServiceType::AppletResource));
        request(&mut mmu, false, 0, 0);
        ipc.handle_sync_request(res, tls, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(mmu.read_u32(tls + 8).unwrap(), 1 << 1);
        let shm = mmu.read_u32(tls + 12).unwrap();
        assert_eq!(ipc.handles.get(&shm), Some(&ServiceType::SharedMemory { size: HID_SHARED_MEMORY_SIZE }));

        // Control: QueryPointerBufferSize.
        request(&mut mmu, false, 3, 0);
        mmu.write_u32(tls, 5).unwrap();
        ipc.handle_sync_request(hid, tls, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(mmu.read_u32(tls + 0x20).unwrap(), POINTER_BUFFER_SIZE);
    }

    #[test]
    fn test_horizon_big_five_services_handshake() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        // 1. set:sys -> GetSystemLanguage (Cmd 3)
        let setsys_h = ipc.allocate_handle(ServiceType::SetSys);
        // Write CMIF In Header: "SFCI" (0x49434653) at TLS + 0x10, cmd_id = 3 at TLS + 0x18
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 3).unwrap();
        let res = ipc.handle_sync_request(setsys_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        assert_eq!(mmu.read_u32(tls_addr + 0x20).unwrap(), 0); // en-US = 0

        // 2. am:appletOE -> OpenApplicationProxy (Cmd 0)
        let applet_h = ipc.allocate_handle(ServiceType::AppletOE);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        let res = ipc.handle_sync_request(applet_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let proxy_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&proxy_h), Some(&ServiceType::ApplicationProxy));

        // 3. vi:m -> GetDisplayService (Cmd 2)
        let vi_h = ipc.allocate_handle(ServiceType::Vi);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 2).unwrap();
        let res = ipc.handle_sync_request(vi_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let disp_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&disp_h), Some(&ServiceType::ApplicationDisplayService));

        // 4. hid -> CreateAppletResource (Cmd 0)
        let hid_h = ipc.allocate_handle(ServiceType::Hid);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        let res = ipc.handle_sync_request(hid_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let res_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&res_h), Some(&ServiceType::AppletResource));

        // 5. fsp-srv -> OpenDataStorageByCurrentProcess (Cmd 200)
        let fsp_h = ipc.allocate_handle(ServiceType::FspSrv);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 200).unwrap();
        let res = ipc.handle_sync_request(fsp_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let storage_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&storage_h), Some(&ServiceType::Storage));
    }

    #[test]
    fn test_horizon_ro_service_dynamic_loading() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        let nro_addr = 0x8100_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(nro_addr, 4096, Permission::READ_WRITE);

        // Construct fake NRO0 header in memory
        mmu.write_bytes(nro_addr + 0x10, b"NRO0").unwrap();
        mmu.write_u32(nro_addr + 4, 0x40).unwrap(); // MOD0 offset
        mmu.write_bytes(nro_addr + 0x40, b"MOD0").unwrap();

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        // 1. GetService("ro:1") via sm:
        let sm_h = ipc.allocate_handle(ServiceType::Sm);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap(); // GetService
        mmu.write_bytes(tls_addr + 0x20, b"ro:1\0\0\0\0").unwrap();
        let res = ipc.handle_sync_request(sm_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let ro_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&ro_h), Some(&ServiceType::Ro));

        // 2. ro:1 -> LoadNro (Cmd 0)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        // Args: pid (0x52), nro_addr, nro_size (0x1000), bss_addr (0), bss_size (0)
        mmu.write_u64(tls_addr + 0x20, 0x52).unwrap();
        mmu.write_u64(tls_addr + 0x28, nro_addr).unwrap();
        mmu.write_u64(tls_addr + 0x30, 0x1000).unwrap();
        let res = ipc.handle_sync_request(ro_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let loaded_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let loaded_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let loaded_addr = ((loaded_hi as u64) << 32) | (loaded_lo as u64);
        assert_eq!(loaded_addr, nro_addr);
    }

    #[test]
    fn test_pl_u_shared_font_service_and_time_clock() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();
        let sm_h = ipc.allocate_handle(ServiceType::Sm);

        // 1. GetService("pl:u") via sm:
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap(); // GetService
        mmu.write_bytes(tls_addr + 0x20, b"pl:u\0\0\0\0").unwrap();
        let res = ipc.handle_sync_request(sm_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let pl_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&pl_h), Some(&ServiceType::Pl));

        // 2. pl:u -> GetSharedFontAddress (Cmd 1)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap();
        let res = ipc.handle_sync_request(pl_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let font_addr_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(font_addr_lo as u64, SHARED_FONT_ADDR);
        // Verify font header is mapped and contains font magic
        let mut font_header = [0u8; 4];
        mmu.read_bytes(SHARED_FONT_ADDR, &mut font_header).unwrap();
        assert_eq!(&font_header, b"OUNC");

        // 3. GetService("time:u") via sm:
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap();
        mmu.write_bytes(tls_addr + 0x20, b"time:u\0\0").unwrap();
        let res = ipc.handle_sync_request(sm_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let time_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&time_h), Some(&ServiceType::Time));

        // 4. time:u -> GetStandardUserSystemClock (Cmd 0)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        let res = ipc.handle_sync_request(time_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let clock_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&clock_h), Some(&ServiceType::SystemClock));

        // 5. SystemClock -> GetCurrentTime (Cmd 0)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        let res = ipc.handle_sync_request(clock_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let time_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let time_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let timestamp = ((time_hi as u64) << 32) | (time_lo as u64);
        assert!(timestamp > 1700000000); // Valid UNIX timestamp (2024+)
    }

    #[test]
    fn test_nvdrv_as_gpu_and_zbc_ioctls() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        let buf_addr = 0x8100_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(buf_addr, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();
        let nvdrv_h = ipc.allocate_handle(ServiceType::Nvdrv);

        // 1. Open "/dev/nvhost-as-gpu" -> returns fd = 2
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap(); // Open
        mmu.write_bytes(tls_addr + 0x20, b"/dev/nvhost-as-gpu\0").unwrap();
        let res = ipc.handle_sync_request(nvdrv_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let fd = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(fd, 2);

        // 2. Ioctl on as-gpu: NVGPU_AS_IOCTL_ALLOC_SPACE (0xC018_4102)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap(); // Ioctl
        mmu.write_u32(tls_addr + 0x20, fd).unwrap();
        mmu.write_u32(tls_addr + 0x24, 0xC018_4102).unwrap();
        mmu.write_u64(tls_addr + 0x28, buf_addr).unwrap();
        mmu.write_u32(tls_addr + 0x30, 8).unwrap();
        let res = ipc.handle_sync_request(nvdrv_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let gpu_va = mmu.read_u64(buf_addr).unwrap();
        assert_eq!(gpu_va, 0x9800_0000);

        // 3. Ioctl on ctrl-gpu: NVGPU_GPU_IOCTL_GET_L2_CACHE_SIZE (0xC018_4714) -> 4MB
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap();
        mmu.write_u32(tls_addr + 0x20, 3).unwrap(); // fd = 3 (ctrl-gpu)
        mmu.write_u32(tls_addr + 0x24, 0xC018_4714).unwrap();
        mmu.write_u64(tls_addr + 0x28, buf_addr).unwrap();
        mmu.write_u32(tls_addr + 0x30, 4).unwrap();
        let res = ipc.handle_sync_request(nvdrv_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let l2_size = mmu.read_u32(buf_addr).unwrap();
        assert_eq!(l2_size, 0x0040_0000); // 4MB L2
    }

    #[test]
    fn test_application_functions_and_self_controller() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        // 1. am:appletOE -> OpenApplicationProxy (Cmd 0)
        let applet_h = ipc.allocate_handle(ServiceType::AppletOE);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        let res = ipc.handle_sync_request(applet_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let proxy_h = mmu.read_u32(tls_addr + 0x20).unwrap();

        // 2. ApplicationProxy -> GetApplicationFunctions (Cmd 20)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 20).unwrap();
        let res = ipc.handle_sync_request(proxy_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let app_func_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&app_func_h), Some(&ServiceType::ApplicationFunctions));

        // 3. ApplicationFunctions -> GetDesiredLanguage (Cmd 21)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 21).unwrap();
        let res = ipc.handle_sync_request(app_func_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let lang = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(lang, 0); // en-US

        // 4. ApplicationProxy -> GetSelfController (Cmd 1)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap();
        let res = ipc.handle_sync_request(proxy_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let self_ctrl_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&self_ctrl_h), Some(&ServiceType::SelfController));

        // 5. SelfController -> CreateManagedDisplaySeparableLayer (Cmd 10)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 10).unwrap();
        let res = ipc.handle_sync_request(self_ctrl_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let layer_id = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(layer_id, 1);
    }

    #[test]
    fn test_fsp_srv_sdmc_and_save_partitions_and_directories() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        let entry_buf = 0x8100_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(entry_buf, 0x2000, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        let fsp_h = ipc.allocate_handle(ServiceType::FspSrv);

        // 1. Mount sdmc:/ partition via fsp-srv OpenSdCardFileSystem (Cmd 18)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 18).unwrap();
        let res = ipc.handle_sync_request(fsp_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let sdmc_fs_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&sdmc_fs_h), Some(&ServiceType::FileSystem("sdmc:/".to_string())));

        // 2. Query free space via IFileSystem (Cmd 11)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 11).unwrap();
        let res = ipc.handle_sync_request(sdmc_fs_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let free_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let free_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let free_bytes = ((free_hi as u64) << 32) | (free_lo as u64);
        assert_eq!(free_bytes, 64 * 1024 * 1024 * 1024);

        // 3. Create directory and files on sdmc:/ via IFileSystem
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 2).unwrap(); // CreateDirectory
        mmu.write_bytes(tls_addr + 0x20, b"/switch/tools\0").unwrap();
        let res = ipc.handle_sync_request(sdmc_fs_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);

        vfs.mount_file("sdmc:/switch/tools/ftpd.nro", b"FTPD_NRO_PAYLOAD");
        vfs.mount_file("sdmc:/switch/tools/sys-clk.nro", b"SYSCLK_NRO_PAYLOAD");

        // 4. OpenDirectory (Cmd 9) on IFileSystem
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 9).unwrap();
        mmu.write_bytes(tls_addr + 0x20, b"/switch/tools\0").unwrap();
        let res = ipc.handle_sync_request(sdmc_fs_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let dir_h = mmu.read_u32(tls_addr + 0x20).unwrap();

        // 5. ReadDirectory (Cmd 0) on IDirectory into guest memory buffer
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap();
        mmu.write_u64(tls_addr + 0x20, entry_buf).unwrap(); // buffer addr
        mmu.write_u64(tls_addr + 0x28, 10).unwrap(); // max entries
        let res = ipc.handle_sync_request(dir_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let count_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(count_lo, 2);

        // Verify entry names written in guest memory buffer
        let mut entry0_name = [0u8; 8];
        mmu.read_bytes(entry_buf, &mut entry0_name).unwrap();
        let name0 = String::from_utf8_lossy(&entry0_name);
        assert!(name0.starts_with("ftpd") || name0.starts_with("sys-"));

        // 6. Mount save:/ partition via fsp-srv OpenSaveDataFileSystem (Cmd 51)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 51).unwrap();
        let res = ipc.handle_sync_request(fsp_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let save_fs_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&save_fs_h), Some(&ServiceType::FileSystem("save:/".to_string())));

        // 7. Direct fsp-srv commands (OpenDirectory Cmd 10 & GetFreeSpaceSize Cmd 12)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 12).unwrap();
        mmu.write_bytes(tls_addr + 0x20, b"save:/\0").unwrap();
        let res = ipc.handle_sync_request(fsp_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let save_free_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let save_free_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        assert_eq!(((save_free_hi as u64) << 32) | (save_free_lo as u64), 64 * 1024 * 1024 * 1024);
    }

    #[test]
    fn test_time_service_monotonic_counter_and_posix_clock() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        let time_h = ipc.allocate_handle(ServiceType::Time);

        // 1. Direct monotonic clock on time:u (Cmd 20)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 20).unwrap();
        let res = ipc.handle_sync_request(time_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let mono_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let mono_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let nanos1 = ((mono_hi as u64) << 32) | (mono_lo as u64);

        // Sleep briefly and query again to verify monotonic advancement
        std::thread::sleep(std::time::Duration::from_millis(2));

        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 20).unwrap();
        let res = ipc.handle_sync_request(time_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let mono_lo2 = mmu.read_u32(tls_addr + 0x20).unwrap();
        let mono_hi2 = mmu.read_u32(tls_addr + 0x24).unwrap();
        let nanos2 = ((mono_hi2 as u64) << 32) | (mono_lo2 as u64);
        assert!(nanos2 > nanos1);

        // 2. Direct POSIX time clock on time:u (Cmd 21)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 21).unwrap();
        let res = ipc.handle_sync_request(time_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let posix_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let posix_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let posix_time = ((posix_hi as u64) << 32) | (posix_lo as u64);
        assert!(posix_time > 1700000000); // Valid UTC epoch timestamp

        // 3. SteadyClock via GetStandardSteadyClock (Cmd 2)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 2).unwrap();
        let res = ipc.handle_sync_request(time_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let steady_h = mmu.read_u32(tls_addr + 0x20).unwrap();
        assert_eq!(ipc.handles.get(&steady_h), Some(&ServiceType::SteadyClock));

        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 0).unwrap(); // GetCurrentTimePoint
        let res = ipc.handle_sync_request(steady_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        let steady_lo = mmu.read_u32(tls_addr + 0x20).unwrap();
        let steady_hi = mmu.read_u32(tls_addr + 0x24).unwrap();
        let steady_nanos = ((steady_hi as u64) << 32) | (steady_lo as u64);
        assert!(steady_nanos >= nanos2);
    }

    #[test]
    fn test_setsys_language_console_model_and_device_nickname() {
        let mut mmu = Memory::new();
        let tls_addr = 0x8000_0000u64;
        let nick_buf = 0x8100_0000u64;
        mmu.map_range(tls_addr, 4096, Permission::READ_WRITE);
        mmu.map_range(nick_buf, 4096, Permission::READ_WRITE);

        let mut ipc = IpcManager::new();
        let mut nvgpu = NvgpuChannel::new();
        let mut audio = AudioOutServer::new();
        let mut vfs = VirtualFileSystem::new();

        let setsys_h = ipc.allocate_handle(ServiceType::SetSys);

        // 1. GetSystemLanguage (Cmd 3) -> en-US = 0
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 3).unwrap();
        let res = ipc.handle_sync_request(setsys_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        assert_eq!(mmu.read_u32(tls_addr + 0x20).unwrap(), 0); // en-US = 0

        // 2. GetConsoleModel / GetProductModel (Cmd 8 and Cmd 67) -> 0x02 (Drake / T239)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 8).unwrap();
        let res = ipc.handle_sync_request(setsys_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        assert_eq!(mmu.read_u32(tls_addr + 0x20).unwrap(), 0x02);

        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 67).unwrap();
        let res = ipc.handle_sync_request(setsys_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);
        assert_eq!(mmu.read_u32(tls_addr + 0x20).unwrap(), 0x02);

        // 3. GetDeviceNickName (Cmd 6) -> "Ounce-T239" written to guest memory buffer
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 6).unwrap();
        mmu.write_u64(tls_addr + 0x20, nick_buf).unwrap(); // pass buf addr
        let res = ipc.handle_sync_request(setsys_h, tls_addr, &mut mmu, &mut nvgpu, &mut audio, &mut vfs);
        assert_eq!(res, 0);

        let mut read_nick = [0u8; 10];
        mmu.read_bytes(nick_buf, &mut read_nick).unwrap();
        assert_eq!(&read_nick, b"Ounce-T239");
    }
}
