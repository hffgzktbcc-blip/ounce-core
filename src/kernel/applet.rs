#![allow(dead_code)]

//! Applet manager services `appletOE` / `appletAE` in the shape libnx's `applet.c` uses them.
//!
//! Covers what `appletInitialize` and `appletMainLoop` need: the proxy for each applet type and its
//! sub-interfaces, `ICommonStateGetter` with the message queue, its event and the focus state,
//! `ISelfController` / `IWindowController` / `IAudioController` setters, and the
//! `IApplicationFunctions` calls libnx makes at startup. Command IDs follow libnx's public source.
//! Values that come from a real system (resource user ID, display version, device ID) are synthetic.

use std::collections::VecDeque;

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// `am` ResultNoMessages (module 128, description 3): `ReceiveMessage` with an empty queue.
pub const RESULT_NO_MESSAGES: u32 = 3 << 9 | 128;
/// `am` ResultNoDataInChannel (module 128, description 2): no launch parameter to pop.
pub const RESULT_NO_DATA_IN_CHANNEL: u32 = 2 << 9 | 128;

/// `AppletMessage` values (libnx `applet.h`).
pub const MESSAGE_EXIT_REQUEST: u32 = 4;
pub const MESSAGE_FOCUS_STATE_CHANGED: u32 = 15;
pub const MESSAGE_OPERATION_MODE_CHANGED: u32 = 30;
pub const MESSAGE_PERFORMANCE_MODE_CHANGED: u32 = 31;

/// `AppletFocusState_InFocus`.
pub const FOCUS_IN_FOCUS: u8 = 1;
/// `AppletOperationMode`: handheld / console (docked).
pub const OPERATION_MODE_HANDHELD: u8 = 0;
pub const OPERATION_MODE_CONSOLE: u8 = 1;

/// Synthetic applet resource user ID handed to the guest (libnx passes it on to `hid` and others).
pub const SYNTHETIC_ARUID: u64 = 0x51;

/// Which applet proxy a session is (the `Open*Proxy` command that created it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyKind {
    Application,
    SystemApplet,
    LibraryApplet,
    OverlayApplet,
    SystemApplication,
}

/// An `am` interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppletInterface {
    /// `appletOE` root.
    OeRoot,
    /// `appletAE` root.
    AeRoot,
    Proxy(ProxyKind),
    CommonStateGetter,
    SelfController,
    WindowController,
    AudioController,
    DisplayController,
    ApplicationFunctions,
    LibraryAppletSelfAccessor,
    /// Interfaces libnx only opens at startup; their commands are not modelled.
    Other(&'static str),
}

/// Applet state shared by every `am` session of the process.
#[derive(Debug, Clone)]
pub struct AppletState {
    /// Pending `AppletMessage`s, oldest first.
    pub messages: VecDeque<u32>,
    /// The `ICommonStateGetter` message event, once handed out.
    pub message_event: Option<u32>,
    pub focus_state: u8,
    pub operation_mode: u8,
    /// `ApmPerformanceMode`: 0 normal (handheld), 1 boost (docked).
    pub performance_mode: u32,
    /// `ISelfController::Exit` was called.
    pub exit_requested: bool,
    pub notified_running: bool,
    /// The guest asked for operation / performance mode change messages.
    pub operation_mode_notifications: bool,
    pub performance_mode_notifications: bool,
}

impl Default for AppletState {
    fn default() -> Self {
        Self {
            messages: VecDeque::new(),
            message_event: None,
            focus_state: FOCUS_IN_FOCUS,
            operation_mode: OPERATION_MODE_HANDHELD,
            performance_mode: 0,
            exit_requested: false,
            notified_running: false,
            operation_mode_notifications: false,
            performance_mode_notifications: false,
        }
    }
}

impl AppletState {
    /// Queues a message for `ReceiveMessage`; the message event reads as signaled until it is drained.
    /// Whoever calls this from outside an IPC request should wake threads waiting on `message_event`.
    pub fn push_message(&mut self, msg: u32) {
        self.messages.push_back(msg);
    }

    /// Asks the guest to exit, the way the system does when the user closes an application.
    pub fn request_exit(&mut self) {
        self.push_message(MESSAGE_EXIT_REQUEST);
    }

    /// Switches handheld / docked and posts the change messages the guest subscribed to.
    pub fn set_operation_mode(&mut self, mode: u8) {
        if mode == self.operation_mode {
            return;
        }
        self.operation_mode = mode;
        self.performance_mode = (mode == OPERATION_MODE_CONSOLE) as u32;
        if self.operation_mode_notifications {
            self.push_message(MESSAGE_OPERATION_MODE_CHANGED);
        }
        if self.performance_mode_notifications {
            self.push_message(MESSAGE_PERFORMANCE_MODE_CHANGED);
        }
    }

    /// `GetDefaultDisplayResolution`: T239 handheld panel 1920x1080, docked 3840x2160.
    pub fn default_display_resolution(&self) -> (u32, u32) {
        if self.operation_mode == OPERATION_MODE_CONSOLE {
            (3840, 2160)
        } else {
            (1920, 1080)
        }
    }
}

/// `in_data` byte `i`, or 0.
fn arg_u32(mmu: &Memory, args: u64) -> u32 {
    mmu.read_u32(args).unwrap_or(0)
}

impl IpcManager {
    fn applet_event(&mut self, name: &'static str) -> u32 {
        self.allocate_handle(ServiceType::ServiceEvent(name))
    }

    pub(super) fn handle_applet(&mut self, iface: AppletInterface, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        use AppletInterface as I;
        let open = |kind| ServiceType::Applet(I::Proxy(kind));
        match iface {
            I::OeRoot => match cmd {
                // OpenApplicationProxy(pid, process handle)
                0 => self.reply_object(tls, mmu, open(ProxyKind::Application)),
                _ => self.reply_unimplemented("appletOE", cmd, tls, mmu),
            },
            I::AeRoot => match cmd {
                100 => self.reply_object(tls, mmu, open(ProxyKind::SystemApplet)),
                // OpenLibraryAppletProxyOld / OpenLibraryAppletProxy (attribute buffer ignored)
                200 | 201 => self.reply_object(tls, mmu, open(ProxyKind::LibraryApplet)),
                300 => self.reply_object(tls, mmu, open(ProxyKind::OverlayApplet)),
                350 => self.reply_object(tls, mmu, open(ProxyKind::SystemApplication)),
                _ => self.reply_unimplemented("appletAE", cmd, tls, mmu),
            },
            I::Proxy(kind) => self.handle_applet_proxy(kind, cmd, tls, mmu),
            I::CommonStateGetter => self.handle_common_state_getter(cmd, args, tls, mmu),
            I::SelfController => self.handle_self_controller(cmd, args, tls, mmu),
            I::WindowController => match cmd {
                // GetAppletResourceUserId
                1 => self.reply_data(tls, mmu, &SYNTHETIC_ARUID.to_le_bytes()),
                // AcquireForegroundRights / ReleaseForegroundRights / RejectToChangeIntoBackground
                10..=12 => self.reply_ok(tls, mmu),
                // SetAppletWindowVisibility(bool) / SetAppletGpuTimeSlice(s64)
                20 | 21 => self.reply_ok(tls, mmu),
                _ => self.reply_unimplemented("IWindowController", cmd, tls, mmu),
            },
            I::AudioController => match cmd {
                // SetExpectedMasterVolume(f32, f32) / ChangeMainAppletMasterVolume / SetTransparentVolumeRate
                0 | 3 | 4 => self.reply_ok(tls, mmu),
                // GetMainAppletExpectedMasterVolume / GetLibraryAppletExpectedMasterVolume -> 1.0
                1 | 2 => self.reply_data(tls, mmu, &1.0f32.to_le_bytes()),
                _ => self.reply_unimplemented("IAudioController", cmd, tls, mmu),
            },
            I::DisplayController => self.reply_unimplemented("IDisplayController", cmd, tls, mmu),
            I::ApplicationFunctions => self.handle_application_functions(cmd, tls, mmu),
            I::LibraryAppletSelfAccessor => match cmd {
                // GetLibraryAppletInfo -> (applet ID, library applet mode). Synthetic: no caller applet.
                11 => self.reply_data(tls, mmu, &[0u8; 8]),
                _ => self.reply_unimplemented("ILibraryAppletSelfAccessor", cmd, tls, mmu),
            },
            I::Other(name) => self.reply_unimplemented(name, cmd, tls, mmu),
        }
    }

    /// `IApplicationProxy` / `ISystemAppletProxy` / `ILibraryAppletProxy` / `IOverlayAppletProxy`.
    fn handle_applet_proxy(&mut self, kind: ProxyKind, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        use AppletInterface as I;
        use ProxyKind as K;
        let iface = match (cmd, kind) {
            (0, _) => I::CommonStateGetter,
            (1, _) => I::SelfController,
            (2, _) => I::WindowController,
            (3, _) => I::AudioController,
            (4, _) => I::DisplayController,
            (10, _) => I::Other("IProcessWindingController"),
            (11, _) => I::Other("ILibraryAppletCreator"),
            (1000, _) => I::Other("IDebugFunctions"),
            (20, K::Application | K::SystemApplication) => I::ApplicationFunctions,
            (20, K::SystemApplet) => I::Other("IHomeMenuFunctions"),
            (20, K::LibraryApplet) => I::LibraryAppletSelfAccessor,
            (20, K::OverlayApplet) => I::Other("IOverlayFunctions"),
            (21, K::SystemApplet) => I::Other("IGlobalStateController"),
            (21, K::LibraryApplet | K::OverlayApplet) => I::Other("IAppletCommonFunctions"),
            (22, K::SystemApplet) => I::Other("IApplicationCreator"),
            (22, K::LibraryApplet) => I::Other("IHomeMenuFunctions"),
            (23, K::SystemApplet) => I::Other("IAppletCommonFunctions"),
            (23, K::LibraryApplet | K::OverlayApplet) => I::Other("IGlobalStateController"),
            _ => return self.reply_unimplemented("IApplicationProxy", cmd, tls, mmu),
        };
        self.reply_object(tls, mmu, ServiceType::Applet(iface))
    }

    fn handle_common_state_getter(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        let s = &self.applet;
        match cmd {
            // GetEventHandle: one event for the process, signaled while messages are queued.
            0 => {
                let ev = match self.applet.message_event {
                    Some(ev) => ev,
                    None => {
                        let ev = self.applet_event("AppletMessage");
                        self.applet.message_event = Some(ev);
                        ev
                    }
                };
                self.reply_event(tls, mmu, ev)
            }
            // ReceiveMessage
            1 => match self.applet.messages.pop_front() {
                Some(msg) => self.reply_data(tls, mmu, &msg.to_le_bytes()),
                None => self.reply_error(tls, mmu, RESULT_NO_MESSAGES),
            },
            // GetOperationMode -> u8
            5 => self.reply_data(tls, mmu, &[s.operation_mode]),
            // GetPerformanceMode -> u32
            6 => self.reply_data(tls, mmu, &s.performance_mode.to_le_bytes()),
            // GetCradleStatus -> u8 (docked means mounted)
            7 => self.reply_data(tls, mmu, &[(s.operation_mode == OPERATION_MODE_CONSOLE) as u8]),
            // GetBootMode -> u8 (normal)
            8 => self.reply_data(tls, mmu, &[0]),
            // GetCurrentFocusState -> u8
            9 => self.reply_data(tls, mmu, &[s.focus_state]),
            // RequestToAcquireSleepLock / ReleaseSleepLock
            10 | 11 => self.reply_ok(tls, mmu),
            // GetAcquiredSleepLockEvent
            13 => {
                let ev = self.applet_event("AcquiredSleepLock");
                self.reply_event(tls, mmu, ev)
            }
            // IsVrModeEnabled -> false
            50 => self.reply_data(tls, mmu, &[0]),
            // SetVrModeEnabled / SetLcdBacklightOffEnabled
            51 | 52 => self.reply_ok(tls, mmu),
            // IsInControllerFirmwareUpdateSection -> false
            55 => self.reply_data(tls, mmu, &[0]),
            // GetDefaultDisplayResolution -> (s32 width, s32 height)
            60 => {
                let (w, h) = s.default_display_resolution();
                let mut out = w.to_le_bytes().to_vec();
                out.extend_from_slice(&h.to_le_bytes());
                self.reply_data(tls, mmu, &out)
            }
            // GetDefaultDisplayResolutionChangeEvent
            61 => {
                let ev = self.applet_event("DefaultDisplayResolutionChange");
                self.reply_event(tls, mmu, ev)
            }
            // SetCpuBoostMode(u32) / CancelCpuBoostMode
            66 | 67 => self.reply_ok(tls, mmu),
            // GetBuiltInDisplayType -> s32
            68 => self.reply_data(tls, mmu, &0u32.to_le_bytes()),
            // SetPerformanceConfigurationChangedNotification(bool)
            90 => self.reply_ok(tls, mmu),
            // GetCurrentPerformanceConfiguration -> u32
            91 => {
                let mode = self.applet.performance_mode;
                self.reply_data(tls, mmu, &self.apm.configuration(mode).to_le_bytes())
            }
            // GetOperationModeSystemInfo -> u32
            200 => self.reply_data(tls, mmu, &0u32.to_le_bytes()),
            _ => {
                let _ = arg_u32(mmu, args);
                self.reply_unimplemented("ICommonStateGetter", cmd, tls, mmu)
            }
        }
    }

    fn handle_self_controller(&mut self, cmd: u32, args: u64, tls: u64, mmu: &mut Memory) -> u64 {
        let flag = mmu.read_u8(args).unwrap_or(0) != 0;
        match cmd {
            // Exit
            0 => {
                self.applet.exit_requested = true;
                self.reply_ok(tls, mmu)
            }
            // LockExit / UnlockExit / EnterFatalSection / LeaveFatalSection
            1..=4 => self.reply_ok(tls, mmu),
            // GetLibraryAppletLaunchableEvent
            9 => {
                let ev = self.applet_event("LibraryAppletLaunchable");
                self.reply_event(tls, mmu, ev)
            }
            // SetScreenShotPermission(u32)
            10 => self.reply_ok(tls, mmu),
            // SetOperationModeChangedNotification(bool)
            11 => {
                self.applet.operation_mode_notifications = flag;
                self.reply_ok(tls, mmu)
            }
            // SetPerformanceModeChangedNotification(bool)
            12 => {
                self.applet.performance_mode_notifications = flag;
                self.reply_ok(tls, mmu)
            }
            // SetFocusHandlingMode(3 x bool) / SetRestartMessageEnabled(bool) / SetOutOfFocusSuspendingEnabled(bool)
            // SetControllerFirmwareUpdateSection / SetRequiresCaptureButtonShortPressedMessage /
            // SetAlbumImageOrientation / SetDesirableKeyboardLayout
            13 | 14 | 16..=20 => self.reply_ok(tls, mmu),
            // CreateManagedDisplayLayer -> u64 layer ID
            40 => self.reply_data(tls, mmu, &1u64.to_le_bytes()),
            // CreateManagedDisplaySeparableLayer -> (u64 display layer, u64 recording layer)
            44 => {
                let mut out = 1u64.to_le_bytes().to_vec();
                out.extend_from_slice(&2u64.to_le_bytes());
                self.reply_data(tls, mmu, &out)
            }
            // SetManagedDisplayLayerSeparationMode / SetHandlesRequestToDisplay / ApproveToDisplay
            45 | 50 | 51 => self.reply_ok(tls, mmu),
            // SetIdleTimeDetectionExtension(u32)
            62 => self.reply_ok(tls, mmu),
            // GetIdleTimeDetectionExtension -> u32
            63 => self.reply_data(tls, mmu, &0u32.to_le_bytes()),
            // ReportUserIsActive / SetAutoSleepDisabled(bool)
            65 | 68 => self.reply_ok(tls, mmu),
            // IsAutoSleepDisabled -> bool
            69 => self.reply_data(tls, mmu, &[0]),
            // GetAccumulatedSuspendedTickValue -> u64 (never suspended)
            90 => self.reply_data(tls, mmu, &0u64.to_le_bytes()),
            // GetAccumulatedSuspendedTickChangedEvent
            91 => {
                let ev = self.applet_event("AccumulatedSuspendedTickChanged");
                self.reply_event(tls, mmu, ev)
            }
            // SetAlbumImageTakenNotificationEnabled(bool)
            100 => self.reply_ok(tls, mmu),
            _ => self.reply_unimplemented("ISelfController", cmd, tls, mmu),
        }
    }

    fn handle_application_functions(&mut self, cmd: u32, tls: u64, mmu: &mut Memory) -> u64 {
        match cmd {
            // PopLaunchParameter: nothing was pushed.
            1 => self.reply_error(tls, mmu, RESULT_NO_DATA_IN_CHANNEL),
            // EnsureSaveData(uid) -> u64 (0 = nothing more needed)
            20 => self.reply_data(tls, mmu, &0u64.to_le_bytes()),
            // GetDesiredLanguage -> u64 language code
            21 => self.reply_data(tls, mmu, &super::set::DEFAULT_LANGUAGE_CODE.to_le_bytes()),
            // SetTerminateResult(u32)
            22 => self.reply_ok(tls, mmu),
            // GetDisplayVersion -> char[0x10]
            23 => {
                let mut out = [0u8; 0x10];
                out[..5].copy_from_slice(b"1.0.0");
                self.reply_data(tls, mmu, &out)
            }
            // NotifyRunning -> bool
            40 => {
                self.applet.notified_running = true;
                self.reply_data(tls, mmu, &[1])
            }
            // GetPseudoDeviceId -> Uuid (synthetic)
            50 => self.reply_data(tls, mmu, b"ounce-pseudo-dev"),
            // SetMediaPlaybackStateForApplication(bool) / EnableApplicationCrashReport(bool)
            60 | 90 => self.reply_ok(tls, mmu),
            // IsGamePlayRecordingSupported -> false
            65 => self.reply_data(tls, mmu, &[0]),
            // GetGpuErrorDetectedSystemEvent
            130 => {
                let ev = self.applet_event("GpuErrorDetected");
                self.reply_event(tls, mmu, ev)
            }
            _ => self.reply_unimplemented("IApplicationFunctions", cmd, tls, mmu),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{Harness, Req};

    /// Opens appletOE the way `appletInitialize` does (domain, PID + process handle) and returns
    /// (root, proxy).
    fn open_application_proxy(h: &mut Harness) -> (u32, u32) {
        let root = h.get_service("appletOE");
        assert_eq!(h.ipc.handles.get(&root), Some(&ServiceType::AppletOE));
        h.ipc.domains.insert(root);
        let r = h.call(root, Req { cmd: 0, data: &[0; 8], pid: true, copy_handles: &[0xFFFF8001], domain_object: Some(root), ..Default::default() });
        assert_eq!(r.result, 0);
        let proxy = r.object();
        assert_eq!(h.ipc.handles.get(&proxy), Some(&ServiceType::Applet(AppletInterface::Proxy(ProxyKind::Application))));
        (root, proxy)
    }

    fn get(h: &mut Harness, root: u32, proxy: u32, cmd: u32) -> u32 {
        let _ = proxy;
        let r = h.call(root, Req { cmd, domain_object: Some(proxy), ..Default::default() });
        assert_eq!(r.result, 0, "proxy command {cmd}");
        r.object()
    }

    #[test]
    fn application_proxy_opens_every_interface_libnx_asks_for() {
        let mut h = Harness::new();
        let (root, proxy) = open_application_proxy(&mut h);
        for (cmd, iface) in [
            (0, AppletInterface::CommonStateGetter),
            (1, AppletInterface::SelfController),
            (2, AppletInterface::WindowController),
            (3, AppletInterface::AudioController),
            (4, AppletInterface::DisplayController),
            (11, AppletInterface::Other("ILibraryAppletCreator")),
            (20, AppletInterface::ApplicationFunctions),
            (1000, AppletInterface::Other("IDebugFunctions")),
        ] {
            let obj = get(&mut h, root, proxy, cmd);
            assert_eq!(h.ipc.handles.get(&obj), Some(&ServiceType::Applet(iface)), "cmd {cmd}");
        }
        // Unknown proxy command: an error, not a fake session.
        let r = h.call(root, Req { cmd: 99, domain_object: Some(proxy), ..Default::default() });
        assert_eq!(r.result, crate::kernel::ipc::RESULT_UNKNOWN_COMMAND);
    }

    #[test]
    fn library_applet_proxy_via_applet_ae() {
        let mut h = Harness::new();
        let root = h.get_service("appletAE");
        let r = h.call(root, Req { cmd: 201, data: &[0; 8], pid: true, copy_handles: &[0xFFFF8001], ..Default::default() });
        let proxy = r.object();
        assert_eq!(h.ipc.handles.get(&proxy), Some(&ServiceType::Applet(AppletInterface::Proxy(ProxyKind::LibraryApplet))));
        for (cmd, iface) in [
            (20, AppletInterface::LibraryAppletSelfAccessor),
            (21, AppletInterface::Other("IAppletCommonFunctions")),
            (22, AppletInterface::Other("IHomeMenuFunctions")),
            (23, AppletInterface::Other("IGlobalStateController")),
            (10, AppletInterface::Other("IProcessWindingController")),
        ] {
            let obj = h.call(proxy, Req { cmd, ..Default::default() }).object();
            assert_eq!(h.ipc.handles.get(&obj), Some(&ServiceType::Applet(iface)), "cmd {cmd}");
        }
        let acc = h.call(proxy, Req { cmd: 20, ..Default::default() }).object();
        assert_eq!(h.call_out(acc, Req { cmd: 11, ..Default::default() }, 8).result, 0);
        for (cmd, kind) in [(100, ProxyKind::SystemApplet), (300, ProxyKind::OverlayApplet), (350, ProxyKind::SystemApplication)] {
            let p = h.call(root, Req { cmd, data: &[0; 8], pid: true, ..Default::default() }).object();
            assert_eq!(h.ipc.handles.get(&p), Some(&ServiceType::Applet(AppletInterface::Proxy(kind))));
        }
    }

    #[test]
    fn common_state_getter_message_queue_event_and_focus() {
        let mut h = Harness::new();
        let (root, proxy) = open_application_proxy(&mut h);
        let csg = get(&mut h, root, proxy, 0);
        let call = |h: &mut Harness, cmd, out| h.call_out(root, Req { cmd, domain_object: Some(csg), ..Default::default() }, out);

        // GetEventHandle: a copy handle, the same one every time.
        let ev = call(&mut h, 0, 0).copy_handles[0];
        assert_eq!(call(&mut h, 0, 0).copy_handles[0], ev);
        assert_eq!(h.ipc.service_event_signaled(ev), Some(false));

        // Empty queue: ResultNoMessages, which appletMainLoop treats as "keep running".
        assert_eq!(call(&mut h, 1, 4).result, RESULT_NO_MESSAGES);

        // A queued message signals the event until it is received.
        h.ipc.applet.request_exit();
        assert_eq!(h.ipc.service_event_signaled(ev), Some(true));
        let r = call(&mut h, 1, 4);
        assert_eq!((r.result, r.u32(0)), (0, MESSAGE_EXIT_REQUEST));
        assert_eq!(h.ipc.service_event_signaled(ev), Some(false));

        // In focus from the start, so appletInitialize skips its focus wait loop.
        assert_eq!(call(&mut h, 9, 1).u8(0), FOCUS_IN_FOCUS);
        assert_eq!(call(&mut h, 5, 1).u8(0), OPERATION_MODE_HANDHELD);
        assert_eq!(call(&mut h, 6, 4).u32(0), 0);
        assert_eq!(call(&mut h, 7, 1).u8(0), 0);
        assert_eq!(call(&mut h, 8, 1).u8(0), 0);
        let r = call(&mut h, 60, 8);
        assert_eq!((r.u32(0), r.u32(4)), (1920, 1080));
        assert_eq!(call(&mut h, 91, 4).u32(0), crate::kernel::apm::CONFIG_NORMAL);
        assert_eq!(call(&mut h, 61, 0).copy_handles.len(), 1);
        assert_eq!(call(&mut h, 13, 0).copy_handles.len(), 1);
        for cmd in [10, 11, 52, 66, 67, 90] {
            assert_eq!(call(&mut h, cmd, 0).result, 0, "cmd {cmd}");
        }
        assert_eq!(call(&mut h, 12345, 0).result, crate::kernel::ipc::RESULT_UNKNOWN_COMMAND);
    }

    #[test]
    fn operation_mode_change_posts_subscribed_messages() {
        let mut h = Harness::new();
        let (root, proxy) = open_application_proxy(&mut h);
        let sc = get(&mut h, root, proxy, 1);
        let csg = get(&mut h, root, proxy, 0);
        for cmd in [11, 12] {
            assert_eq!(h.call(root, Req { cmd, data: &[1, 0, 0, 0], domain_object: Some(sc), ..Default::default() }).result, 0);
        }
        h.ipc.applet.set_operation_mode(OPERATION_MODE_CONSOLE);
        assert_eq!(h.ipc.applet.messages, [MESSAGE_OPERATION_MODE_CHANGED, MESSAGE_PERFORMANCE_MODE_CHANGED]);
        let r = h.call_out(root, Req { cmd: 5, domain_object: Some(csg), ..Default::default() }, 1);
        assert_eq!(r.u8(0), OPERATION_MODE_CONSOLE);
        let r = h.call_out(root, Req { cmd: 60, domain_object: Some(csg), ..Default::default() }, 8);
        assert_eq!((r.u32(0), r.u32(4)), (3840, 2160));
    }

    #[test]
    fn self_window_audio_controllers_and_application_functions() {
        let mut h = Harness::new();
        let (root, proxy) = open_application_proxy(&mut h);
        let sc = get(&mut h, root, proxy, 1);
        let wc = get(&mut h, root, proxy, 2);
        let ac = get(&mut h, root, proxy, 3);
        let af = get(&mut h, root, proxy, 20);
        let mut call = |obj, cmd, out| h.call_out(root, Req { cmd, data: &[0; 8], domain_object: Some(obj), ..Default::default() }, out);

        // ISelfController: setters libnx calls during init succeed; layers and events come back.
        for cmd in [1, 2, 3, 4, 10, 13, 14, 16, 17, 18, 19, 20, 45, 50, 51, 62, 65, 68, 100] {
            assert_eq!(call(sc, cmd, 0).result, 0, "ISelfController {cmd}");
        }
        assert_eq!(call(sc, 40, 8).u64(0), 1);
        let r = call(sc, 44, 16);
        assert_eq!((r.u64(0), r.u64(8)), (1, 2));
        assert_eq!(call(sc, 90, 8).u64(0), 0);
        assert_eq!(call(sc, 91, 0).copy_handles.len(), 1);
        assert_eq!(call(sc, 9, 0).copy_handles.len(), 1);
        assert_eq!(call(sc, 63, 4).u32(0), 0);
        assert_eq!(call(sc, 69, 1).u8(0), 0);

        // IWindowController
        assert_eq!(call(wc, 1, 8).u64(0), SYNTHETIC_ARUID);
        for cmd in [10, 11, 12, 20, 21] {
            assert_eq!(call(wc, cmd, 0).result, 0, "IWindowController {cmd}");
        }

        // IAudioController
        assert_eq!(f32::from_le_bytes(call(ac, 1, 4).data[..4].try_into().unwrap()), 1.0);
        assert_eq!(f32::from_le_bytes(call(ac, 2, 4).data[..4].try_into().unwrap()), 1.0);
        for cmd in [0, 3, 4] {
            assert_eq!(call(ac, cmd, 0).result, 0, "IAudioController {cmd}");
        }

        // IApplicationFunctions
        assert_eq!(call(af, 1, 0).result, RESULT_NO_DATA_IN_CHANNEL);
        assert_eq!(call(af, 20, 8).u64(0), 0);
        assert_eq!(&call(af, 21, 8).data, b"en-US\0\0\0");
        assert_eq!(call(af, 22, 0).result, 0);
        assert_eq!(&call(af, 23, 5).data, b"1.0.0");
        assert_eq!(call(af, 40, 1).u8(0), 1);
        assert_eq!(call(af, 50, 16).data.len(), 16);
        assert_eq!(call(af, 65, 1).u8(0), 0);
        assert_eq!(call(af, 130, 0).copy_handles.len(), 1);
        assert!(h.ipc.applet.notified_running);

        // Exit
        let r = h.call(root, Req { cmd: 0, domain_object: Some(sc), ..Default::default() });
        assert_eq!(r.result, 0);
        assert!(h.ipc.applet.exit_requested);
    }

    /// The harness's inline frames (no HIPC header) keep the older handlers in ipc.rs.
    #[test]
    fn inline_harness_frames_keep_legacy_behaviour() {
        let mut h = Harness::new();
        let csg = h.ipc.allocate_handle(ServiceType::CommonStateGetter);
        let tls = crate::kernel::cmif::test_util::TLS;
        for i in 0..0x40 {
            h.mmu.write_u32(tls + i * 4, 0).unwrap();
        }
        h.mmu.write_u32(tls + 0x10, crate::kernel::ipc::CMIF_IN_MAGIC).unwrap();
        h.mmu.write_u32(tls + 0x18, 1).unwrap();
        let mut nv = crate::gpu::nvgpu::NvgpuChannel::new();
        let mut au = crate::audio::AudioOutServer::new();
        let mut fs = crate::fs::VirtualFileSystem::new();
        h.ipc.handle_sync_request(csg, tls, &mut h.mmu, &mut nv, &mut au, &mut fs);
        assert_eq!(h.mmu.read_u32(tls + 0x20).unwrap(), 1);
    }
}
