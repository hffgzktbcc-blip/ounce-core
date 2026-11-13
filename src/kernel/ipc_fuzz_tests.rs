//! Bounded fuzz tests for the HIPC parser and the CMIF dispatcher. A guest controls every byte of
//! its message buffer, so any message must produce a reply or a result code, never a panic or a hang.
//! See `loader::fuzz_tests` for the harness (`OUNCE_FUZZ_CASES` sets the number of cases).

use super::applet::AppletInterface;
use super::hipc::{self, Buffer};
use super::ipc::{IpcManager, ServiceType, CMIF_IN_MAGIC};
use crate::audio::AudioOutServer;
use crate::fs::VirtualFileSystem;
use crate::gpu::nvgpu::NvgpuChannel;
use crate::loader::fuzz_tests::{fuzz, Rng};
use crate::mmu::{Memory, Permission};

const TLS: u64 = 0x8000_0000;
const MSG_SIZE: usize = 0x200;
const BUF: u64 = 0x9000_0000;

fn services() -> Vec<ServiceType> {
    use ServiceType::*;
    vec![
        Sm, Nvdrv, Vi, ApplicationDisplayService, Hid, AppletResource, FspSrv, Storage,
        FileSystem("sdmc:/".into()),
        Directory { path: "romfs:/".into(), entries: vec!["a".into()], index: 0 },
        File(1), AudOut, AudRen, AppletOE, ApplicationProxy, CommonStateGetter, SelfController,
        ApplicationFunctions, SetSys, Ro, Pl, Time, SystemClock, SteadyClock, Prepo, Bcat,
        Generic("fuzz:".into()), SharedMemory { size: 0x1000 }, TransferMemory { addr: BUF, size: 0x1000 },
        NvEvent { fd: 1, event_id: 0 }, AudioRenderer, AudioRendererEvent,
        AppletAE, Applet(AppletInterface::OeRoot), Applet(AppletInterface::AeRoot), Applet(AppletInterface::CommonStateGetter),
        Applet(AppletInterface::SelfController), Applet(AppletInterface::WindowController), Applet(AppletInterface::AudioController),
        Applet(AppletInterface::DisplayController), Applet(AppletInterface::ApplicationFunctions),
        Applet(AppletInterface::LibraryAppletSelfAccessor), Applet(AppletInterface::Other("fuzz")),
        Set, TimeZoneService, Psm, PsmSession, Apm, ApmSession, ServiceEvent("fuzz"),
    ]
}

fn memory(message: &[u8]) -> Memory {
    let mut mmu = Memory::new();
    mmu.map_range(TLS, 0x1000, Permission::READ_WRITE);
    mmu.map_range(BUF, 0x10000, Permission::READ_WRITE);
    let mut rng = Rng::new(0xB0F);
    let noise: Vec<u8> = (0..0x10000).map(|_| rng.next() as u8).collect();
    mmu.write_bytes(BUF, &noise).unwrap();
    mmu.write_bytes(TLS, message).unwrap();
    mmu
}

/// A request header with one send and one receive buffer, then a CMIF command with `args` words.
fn seed(cmd: u32, kind: u16, args: &[u32]) -> Vec<u8> {
    let mut w = vec![
        kind as u32 | 1 << 20 | 1 << 24, // one A buffer, one B buffer
        (6 + args.len() as u32 + 4) | 0x2 << 10, // data words, one receive-list entry
        0x100, BUF as u32, 0, // send buffer: size, addr, flags
        0x100, (BUF + 0x1000) as u32, 0, // receive buffer
    ];
    while w.len() % 4 != 0 {
        w.insert(8, 0); // pad so the CMIF payload sits where the parser aligns it
    }
    w.extend([CMIF_IN_MAGIC, 0, cmd, 0]);
    w.extend(args);
    let mut bytes: Vec<u8> = w.iter().flat_map(|v| v.to_le_bytes()).collect();
    bytes.resize(MSG_SIZE, 0);
    bytes
}

#[test]
fn fuzz_hipc_parse_request() {
    let seeds: Vec<Vec<u8>> = (0..4).map(|c| seed(c, hipc::TYPE_REQUEST, &[1, 2, 3])).collect();
    fuzz("hipc", 0x4849_5043, &seeds, |bytes| {
        let mmu = memory(bytes);
        let req = hipc::parse_request(&mmu, TLS);
        let _ = hipc::read_domain_header(&mmu, TLS + req.cmif_offset);
        // Counts are bounded by the 4 and 10 bit header fields, so the parse stays small.
        assert!(req.send_buffers.len() <= 15 && req.recv_list.len() <= 15);
    });
    // A message at the end of the address space must not overflow the offset math either.
    let mut mmu = Memory::new();
    mmu.map_range(u64::MAX - 0xFFF, 0x1000, Permission::READ_WRITE);
    let _ = hipc::parse_request(&mmu, u64::MAX - 0x20);
    let _ = hipc::parse_request(&Memory::new(), u64::MAX);
}

#[test]
fn fuzz_cmif_dispatch_all_services() {
    let mut seeds = Vec::new();
    for cmd in [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 100, 101, 1000] {
        for kind in [hipc::TYPE_REQUEST, hipc::TYPE_CONTROL, hipc::TYPE_REQUEST_WITH_CONTEXT, hipc::TYPE_CLOSE] {
            seeds.push(seed(cmd, kind, &[0x10, BUF as u32, 0x100, 0xFFFF_FFFF]));
        }
    }
    let _ = Buffer::default();
    fuzz("cmif", 0x434D_4946, &seeds, |bytes| {
        for (i, service) in services().into_iter().enumerate() {
            if std::env::var_os("OUNCE_FUZZ_TRACE").is_some() {
                eprintln!("  service {i}: {service:?}");
            }
            let mut mmu = memory(bytes);
            let mut ipc = IpcManager::new();
            let handle = ipc.allocate_handle(service);
            // Every third service is reached through a domain, the others directly.
            if i % 3 == 0 {
                ipc.domains.insert(handle);
            }
            let (mut gpu, mut audio, mut vfs) = (NvgpuChannel::new(), AudioOutServer::new(), VirtualFileSystem::new());
            let _ = ipc.handle_sync_request(handle, TLS, &mut mmu, &mut gpu, &mut audio, &mut vfs);
            // A second request on the same state reaches paths that need an earlier call (open files, renderer).
            let _ = ipc.handle_sync_request(handle, TLS, &mut mmu, &mut gpu, &mut audio, &mut vfs);
        }
    });
}
