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
    /// Generic stubbed service
    Generic(String),
}

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

/// Manages Horizon OS IPC handles and routes HIPC/CMIF messages to HLE services.
pub struct IpcManager {
    pub handles: HashMap<u32, ServiceType>,
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

        let service = match self.handles.get(&session_handle).cloned() {
            Some(s) => s,
            None => return 0xF601, // Invalid handle result code
        };

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

        // CMIF payload starts at TLS + 0x10
        let cmif_base = tls_addr + 0x10;
        let magic = mmu.read_u32(cmif_base).unwrap_or(0);
        if magic != CMIF_IN_MAGIC {
            // Write generic success even if raw HIPC control
            self.write_cmif_response(tls_addr, mmu, 0, &[], None);
            return 0;
        }

        let cmd_id = mmu.read_u32(cmif_base + 8).unwrap_or(0);
        let args_base = cmif_base + 0x10;

        match service {
            ServiceType::Sm => match cmd_id {
                // Command 0: Initialize
                0 => {
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
                        "set:sys" => ServiceType::SetSys,
                        "ro:1" | "ro:dmnt" | "ldr:ro" => ServiceType::Ro,
                        "pl:u" | "pl:s" => ServiceType::Pl,
                        "time:u" | "time:s" => ServiceType::Time,
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
                // Command 0: OpenAudioRenderer -> returns IAudioRenderer session handle
                0 => {
                    let renderer_h = self.allocate_handle(ServiceType::AudRen);
                    self.write_cmif_response(tls_addr, mmu, 0, &[renderer_h], Some(renderer_h));
                    0
                }
                // Command 1: GetAudioRendererWorkBufferSize -> returns 0x40000 (256 KB)
                1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x40000], None);
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

            ServiceType::ApplicationDisplayService => match cmd_id {
                // Command 1010: OpenDisplay("Default") -> returns DisplayId = 0
                1010 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0, 0], None);
                    0
                }
                // Command 2030: CreateStrayLayer -> returns LayerId = 1, IGraphicBufferProducer = 0x100
                2030 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1, 0, 0x100], None);
                    0
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
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::AppletResource => match cmd_id {
                // Command 0: GetSharedMemoryHandle -> returns handle 0x200 (HID Shared Mem)
                0 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0x200], Some(0x200));
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::FspSrv => match cmd_id {
                // Command 0 / 1 / 200: OpenDataStorageByCurrentProcess / OpenDataStorageByProgramId -> returns IStorage
                0 | 1 | 200 => {
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

            ServiceType::FileSystem(mount_prefix) => match cmd_id {
                // Command 0: CreateFile(u32 mode, u64 size, string path)
                0 => {
                    let path = read_cmif_string(mmu, args_base + 16, 256);
                    let full_path = if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    vfs.mount_file(&full_path, &[]);
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 1: DeleteFile(string path)
                1 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let full_path = if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    let res = match vfs.delete_file(&full_path) {
                        Ok(_) => 0,
                        Err(_) => 0x0005_0202,
                    };
                    self.write_cmif_response(tls_addr, mmu, res, &[0], None);
                    0
                }
                // Command 2: CreateDirectory(string path)
                2 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let full_path = if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    let res = match vfs.create_directory(&full_path) {
                        Ok(_) => 0,
                        Err(_) => 0x0005_0202,
                    };
                    self.write_cmif_response(tls_addr, mmu, res, &[0], None);
                    0
                }
                // Command 3 | 4: DeleteDirectory / DeleteDirectoryRecursively(string path)
                3 | 4 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let full_path = if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    let res = match vfs.delete_directory(&full_path) {
                        Ok(_) => 0,
                        Err(_) => 0x0005_0202,
                    };
                    self.write_cmif_response(tls_addr, mmu, res, &[0], None);
                    0
                }
                // Command 9: OpenFile(string path, u32 mode) -> returns IFile
                9 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let full_path = if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    match vfs.open_file(&full_path) {
                        Ok(fh) => {
                            let file_h = self.allocate_handle(ServiceType::File(fh));
                            self.write_cmif_response(tls_addr, mmu, 0, &[file_h], Some(file_h));
                        }
                        Err(_) => {
                            self.write_cmif_response(tls_addr, mmu, 0x0005_0202, &[], None);
                        }
                    }
                    0
                }
                // Command 10: OpenDirectory(string path, u32 filter_flags) -> returns IDirectory
                10 => {
                    let path = read_cmif_string(mmu, args_base, 256);
                    let full_path = if path.is_empty() || path == "/" {
                        mount_prefix.clone()
                    } else if path.contains(":/") {
                        path
                    } else {
                        format!("{}{}", mount_prefix, path.trim_start_matches('/'))
                    };
                    let entries = vfs.list_directory(&full_path).unwrap_or_default();
                    let dir_h = self.allocate_handle(ServiceType::Directory {
                        path: full_path,
                        entries,
                        index: 0,
                    });
                    self.write_cmif_response(tls_addr, mmu, 0, &[dir_h], Some(dir_h));
                    0
                }
                // Command 11: Commit
                11 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
                // Command 12: GetFreeSpaceSize -> returns u64
                12 => {
                    let free_bytes = vfs.get_free_space_size(&mount_prefix);
                    let lo = (free_bytes & 0xFFFF_FFFF) as u32;
                    let hi = ((free_bytes >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 13: GetTotalSpaceSize -> returns u64
                13 => {
                    let total_bytes = vfs.get_total_space_size(&mount_prefix);
                    let lo = (total_bytes & 0xFFFF_FFFF) as u32;
                    let hi = ((total_bytes >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::Directory { path, entries, mut index } => match cmd_id {
                // Command 0: ReadDirectory(out_buf_addr: u64, max_entries: u64) -> count: u64
                0 => {
                    let buf_addr = mmu.read_u64(args_base).unwrap_or(0);
                    let max_entries = mmu.read_u64(args_base + 8).unwrap_or(0) as usize;
                    let count = if max_entries > 0 {
                        (entries.len() - index).min(max_entries)
                    } else {
                        entries.len() - index
                    };

                    if buf_addr >= 0x10000 && mmu.is_mapped(buf_addr) {
                        for i in 0..count {
                            let entry_name = &entries[index + i];
                            let entry_addr = buf_addr + (i * 0x310) as u64;
                            let mut entry_buf = [0u8; 768];
                            let name_bytes = entry_name.as_bytes();
                            let l = name_bytes.len().min(767);
                            entry_buf[..l].copy_from_slice(&name_bytes[..l]);
                            let _ = mmu.write_bytes(entry_addr, &entry_buf);
                            let is_dir = entry_name.contains('/') || !entry_name.contains('.');
                            let _ = mmu.write_u32(entry_addr + 0x304, if is_dir { 0 } else { 1 });
                        }
                    }
                    index += count;
                    self.handles.insert(session_handle, ServiceType::Directory { path, entries, index });

                    let lo = (count as u64 & 0xFFFF_FFFF) as u32;
                    let hi = ((count as u64 >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                // Command 1: GetEntryCount -> count: u64
                1 => {
                    let count = entries.len() as u64;
                    let lo = (count & 0xFFFF_FFFF) as u32;
                    let hi = ((count >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

            ServiceType::File(handle) => match cmd_id {
                // Command 4: GetSize -> u64
                4 => {
                    let sz = vfs.get_file_size(handle).unwrap_or(0) as u64;
                    let lo = (sz & 0xFFFF_FFFF) as u32;
                    let hi = ((sz >> 32) & 0xFFFF_FFFF) as u32;
                    self.write_cmif_response(tls_addr, mmu, 0, &[lo, hi], None);
                    0
                }
                _ => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                    0
                }
            },

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

            ServiceType::Generic(_) => {
                // Stubbed HLE service response: ResultSuccess (0)
                self.write_cmif_response(tls_addr, mmu, 0, &[0], None);
                0
            }
        }
    }

    /// Writes a standard CMIF ("SFCO") response frame back into the thread's TLS buffer.
    fn write_cmif_response(
        &self,
        tls_addr: u64,
        mmu: &mut Memory,
        result_code: u32,
        out_words: &[u32],
        move_handle: Option<u32>,
    ) {
        // HIPC Header at TLS + 0x00
        let has_handle = move_handle.is_some();
        let word0 = if has_handle { 0x80000004 } else { 0x00000004 };
        let word1 = (4 + out_words.len()) as u32;
        let _ = mmu.write_u32(tls_addr, word0);
        let _ = mmu.write_u32(tls_addr + 4, word1);

        if let Some(h) = move_handle {
            // Special header descriptor: 1 move handle at TLS + 0x0C
            let _ = mmu.write_u32(tls_addr + 8, 1 << 4);
            let _ = mmu.write_u32(tls_addr + 12, h);
        }

        // CMIF Out Header ("SFCO") at TLS + 0x10
        let cmif_base = tls_addr + 0x10;
        let _ = mmu.write_u32(cmif_base, CMIF_OUT_MAGIC);
        let _ = mmu.write_u32(cmif_base + 4, 0);
        let _ = mmu.write_u32(cmif_base + 8, result_code);
        let _ = mmu.write_u32(cmif_base + 12, 0);

        for (i, &w) in out_words.iter().enumerate() {
            let _ = mmu.write_u32(cmif_base + 16 + (i * 4) as u64, w);
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

        // 5. fsp-srv -> OpenDataStorageByCurrentProcess (Cmd 1)
        let fsp_h = ipc.allocate_handle(ServiceType::FspSrv);
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 1).unwrap();
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

        // 2. Query free space via IFileSystem (Cmd 12)
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 12).unwrap();
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

        // 4. OpenDirectory (Cmd 10) on IFileSystem
        mmu.write_u32(tls_addr + 0x10, CMIF_IN_MAGIC).unwrap();
        mmu.write_u32(tls_addr + 0x18, 10).unwrap();
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
