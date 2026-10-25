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
    /// System Settings ("set:sys" / "set")
    SetSys,
    /// Relocatable Objects / Dynamic Linker ("ro:1" / "ro:dmnt" / "ldr:ro")
    Ro,
    /// Generic stubbed service
    Generic(String),
}

/// Manages Horizon OS IPC handles and routes HIPC/CMIF messages to HLE services.
pub struct IpcManager {
    pub handles: HashMap<u32, ServiceType>,
    next_handle: u32,
    pub total_ipc_requests: u64,
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
        _vfs: &mut VirtualFileSystem,
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
                // Command 0: Open -> returns (fd = 1, error = 0)
                0 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1, 0], None);
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
                // Command 16: AcquireForegroundRights -> returns ResultSuccess (0)
                16 => {
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
                // Command 0 / 1: GetAvailableLanguageCodes -> returns count=1, code=1 (en-US)
                0 | 1 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1, 0x01], None);
                    0
                }
                // Command 2: GetRegionCode -> returns 1 (USA / NorthAmerica)
                2 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
                // Command 3: GetSystemLanguage -> returns 1 (en-US)
                3 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
                    0
                }
                // Command 4: GetFirmwareVersion -> returns Switch 2 Firmware 23.0.1 ("Ounce" / ns2jb target)
                4 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[23, 0, 1], None);
                    0
                }
                // Command 5: GetColorSetId -> returns 1 (BasicBlack)
                5 => {
                    self.write_cmif_response(tls_addr, mmu, 0, &[1], None);
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
        assert_eq!(mmu.read_u32(tls_addr + 0x20).unwrap(), 1); // en-US

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
}
