//! libnx audio device sessions. Input is deterministic silence in the headless runtime.

use super::ipc::{IpcManager, ServiceType};
use crate::audio::AudioOutServer;
use crate::mmu::Memory;
use std::collections::VecDeque;

const INVALID_ARGUMENT: u32 = 0xF601;
const MAX_BUFFER_BYTES: u64 = 16 * 1024 * 1024;

/// First 64 manager open attempts, retained for bounded failure diagnosis.
#[derive(Debug, Clone)]
pub(crate) struct AudioOpenTrace {
    pub input: bool,
    pub rate: u32,
    pub raw_channels: u32,
    pub result: u32,
    pub channels: Option<u32>,
}

pub(super) struct AudioDevice {
    input: bool,
    playing: bool,
    released: VecDeque<u64>,
}

impl IpcManager {
    pub(super) fn handle_audio_device(
        &mut self,
        service: &ServiceType,
        cmd: u32,
        args: u64,
        tls: u64,
        mmu: &mut Memory,
        audio: &mut AudioOutServer,
    ) -> Option<u64> {
        match service {
            ServiceType::AudIn | ServiceType::AudOut if self.is_libnx_request() => {
                let input = matches!(service, ServiceType::AudIn);
                if !matches!(cmd, 1 | 3) {
                    return Some(self.reply_unimplemented("IAudioDeviceManager", cmd, tls, mmu));
                }
                let rate = mmu.read_u32(args).unwrap_or(0);
                let channels = mmu.read_u32(args + 4).unwrap_or(0);
                // Record both sides of the negotiation before replying, not an inferred success.
                let accepted = matches!(rate, 0 | 48000) && matches!(channels, 0 | 2 | 0x20000);
                if self.audio_open_trace.len() < 64 {
                    self.audio_open_trace.push(AudioOpenTrace {
                        input, rate, raw_channels: channels,
                        result: if accepted { 0 } else { INVALID_ARGUMENT },
                        channels: accepted.then_some(2),
                    });
                }
                // libnx's default channel parameter packs 2 in the upper halfword.
                if !accepted {
                    return Some(self.reply_error(tls, mmu, INVALID_ARGUMENT));
                }
                let id = self.allocate_handle(ServiceType::AudioDevice(0));
                self.handles.insert(id, ServiceType::AudioDevice(id));
                self.audio_devices.insert(
                    id,
                    AudioDevice {
                        input,
                        playing: false,
                        released: VecDeque::new(),
                    },
                );
                self.write_out_buffer(
                    mmu,
                    if input {
                        b"BuiltInHeadset\0"
                    } else {
                        b"DeviceOut\0"
                    },
                );
                let data: Vec<u8> = [48000u32, 2, 2, 1]
                    .into_iter()
                    .flat_map(u32::to_le_bytes)
                    .collect();
                self.reply(tls, mmu, 0, &data, &[], Some(id));
                Some(0)
            }
            ServiceType::AudioDevice(id) => {
                Some(self.audio_device_command(*id, cmd, args, tls, mmu, audio))
            }
            _ => None,
        }
    }

    fn audio_device_command(
        &mut self,
        id: u32,
        cmd: u32,
        args: u64,
        tls: u64,
        mmu: &mut Memory,
        audio: &mut AudioOutServer,
    ) -> u64 {
        let Some(device) = self.audio_devices.get(&id) else {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        };
        let input = device.input;
        match cmd {
            0 => self.reply_data(tls, mmu, &u32::from(!device.playing).to_le_bytes()),
            1 | 2 => {
                self.audio_devices.get_mut(&id).unwrap().playing = cmd == 1;
                self.reply_ok(tls, mmu)
            }
            4 => {
                let event = self.allocate_handle(ServiceType::AudioDeviceEvent(id));
                self.reply_event(tls, mmu, event)
            }
            3 | 7 if !input => self.append_audio_device_buffer(id, args, tls, mmu, audio),
            3 | 8 if input => self.append_audio_device_buffer(id, args, tls, mmu, audio),
            5 | 8 if !input => self.release_audio_device_buffers(id, tls, mmu),
            5 | 9 if input => self.release_audio_device_buffers(id, tls, mmu),
            _ => self.reply_unimplemented("IAudioDevice", cmd, tls, mmu),
        }
    }

    fn append_audio_device_buffer(
        &mut self,
        id: u32,
        args: u64,
        tls: u64,
        mmu: &mut Memory,
        audio: &mut AudioOutServer,
    ) -> u64 {
        let descriptor = self
            .request
            .send_buffers
            .iter()
            .chain(self.request.send_statics.iter().map(|(_, b)| b))
            .find(|buffer| buffer.size >= 40)
            .copied();
        let Some(descriptor) = descriptor else {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        };
        let mut bytes = [0; 40];
        if mmu.read_bytes(descriptor.addr, &mut bytes).is_err() {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        }
        let word = |index| u64::from_le_bytes(bytes[index..index + 8].try_into().unwrap());
        let (address, capacity, size, offset) = (word(8), word(16), word(24), word(32));
        let tag = mmu.read_u64(args).unwrap_or(0);
        if size > MAX_BUFFER_BYTES
            || size % 4 != 0
            || offset.checked_add(size).is_none_or(|end| end > capacity)
            || address
                .checked_add(offset)
                .and_then(|start| start.checked_add(size))
                .is_none()
            || self.audio_devices[&id].released.len() >= 64
        {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        }
        let start = address + offset;
        let mut samples = vec![0; size as usize];
        let valid = if self.audio_devices[&id].input {
            mmu.write_bytes(start, &samples).is_ok()
        } else {
            mmu.read_bytes(start, &mut samples).is_ok()
        };
        if !valid {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        }
        if !self.audio_devices[&id].input {
            audio.append_pcm_buffer(mmu, start, (size / 2) as usize);
        }
        self.audio_devices
            .get_mut(&id)
            .unwrap()
            .released
            .push_back(tag);
        self.reply_ok(tls, mmu)
    }

    fn release_audio_device_buffers(&mut self, id: u32, tls: u64, mmu: &mut Memory) -> u64 {
        let buffer = self
            .request
            .recv_buffers
            .iter()
            .chain(&self.request.recv_list)
            .find(|b| b.size >= 8)
            .copied();
        let Some(buffer) = buffer else {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        };
        let device = self.audio_devices.get_mut(&id).unwrap();
        let count = device.released.len().min((buffer.size / 8) as usize);
        let data: Vec<u8> = device
            .released
            .iter()
            .take(count)
            .flat_map(|tag| tag.to_le_bytes())
            .collect();
        if mmu.write_bytes(buffer.addr, &data).is_err() {
            return self.reply_error(tls, mmu, INVALID_ARGUMENT);
        }
        device.released.drain(..count);
        self.reply_data(tls, mmu, &(count as u32).to_le_bytes())
    }

    pub(super) fn audio_device_event_signaled(&self, id: u32) -> bool {
        self.audio_devices
            .get(&id)
            .is_some_and(|device| !device.released.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{BUF, Harness, Req};

    #[test]
    fn audio_device_sessions_release_buffers_and_reject_invalid_ranges() {
        for service in [ServiceType::AudIn, ServiceType::AudOut] {
            let input = service == ServiceType::AudIn;
            let mut h = Harness::new();
            let root = h.ipc.allocate_handle(service);
            let args: Vec<u8> = [48000u32, 0x20000, 0, 0]
                .into_iter()
                .flat_map(u32::to_le_bytes)
                .collect();
            let opened = h.call_out(
                root,
                Req {
                    cmd: 3,
                    data: &args,
                    recv: Some((BUF, 256)),
                    ..Default::default()
                },
                16,
            );
            assert_eq!(opened.result, 0);
            assert_eq!(opened.u32(0), 48000);
            assert_eq!(opened.u32(12), 1);
            let device = opened.move_handles[0];
            let event = h
                .call(
                    device,
                    Req {
                        cmd: 4,
                        ..Default::default()
                    },
                )
                .copy_handles[0];
            assert_eq!(h.ipc.service_event_signaled(event), Some(false));
            assert_eq!(
                h.call(
                    device,
                    Req {
                        cmd: 1,
                        ..Default::default()
                    }
                )
                .result,
                0
            );
            assert_eq!(h.call_out(device, Req::default(), 4).u32(0), 0);
            let tag = BUF.to_le_bytes();
            let descriptor: Vec<u8> = [0u64, BUF + 128, 16, 16, 0]
                .into_iter()
                .flat_map(u64::to_le_bytes)
                .collect();
            h.mmu.write_bytes(BUF, &descriptor).unwrap();
            h.mmu.write_bytes(BUF + 128, &[0x55; 16]).unwrap();
            let cmd = if input { 8 } else { 7 };
            let request = || Req {
                cmd,
                data: &tag,
                send: Some((BUF, 40)),
                ..Default::default()
            };
            assert_eq!(h.call(device, request()).result, 0);
            assert_eq!(h.ipc.service_event_signaled(event), Some(true));
            if input {
                assert_eq!(h.mmu.read_u32(BUF + 128).unwrap(), 0);
            }
            h.mmu.write_u64(BUF + 24, 20).unwrap();
            assert_ne!(h.call(device, request()).result, 0);
            let released = h.call_out(
                device,
                Req {
                    cmd: if input { 9 } else { 8 },
                    recv: Some((BUF + 64, 8)),
                    ..Default::default()
                },
                4,
            );
            assert_eq!(released.u32(0), 1);
            assert_eq!(h.mmu.read_u64(BUF + 64).unwrap(), BUF);
            assert_eq!(h.ipc.service_event_signaled(event), Some(false));
            let device_id = match h.ipc.handles[&device] {
                ServiceType::AudioDevice(id) => id,
                _ => unreachable!(),
            };
            let clone = h.ipc.allocate_handle(ServiceType::AudioDevice(device_id));
            h.ipc.close_handle(device);
            h.ipc.close_handle(event);
            assert!(h.ipc.audio_devices.contains_key(&device_id));
            h.ipc.close_handle(clone);
            assert!(!h.ipc.audio_devices.contains_key(&device_id));
        }
    }
}
