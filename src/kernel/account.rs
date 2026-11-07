//! Offline local account and its preselected-user launch parameter.
//! Command layouts follow switchbrew/libnx `acc.c` and `applet.c`.

use super::ipc::{IpcManager, ServiceType};
use crate::mmu::Memory;

/// Runtime-local identity, with no Nintendo account or network credentials.
pub const LOCAL_USER: [u8; 16] = 1u128.to_le_bytes();
const INVALID_ARGUMENT: u32 = 0xF601;

fn launch_parameter() -> [u8; 0x88] {
    let mut data = [0; 0x88];
    data[..4].copy_from_slice(&0xc794_97cau32.to_le_bytes());
    data[4] = 1;
    data[8..24].copy_from_slice(&LOCAL_USER);
    data
}

impl IpcManager {
    pub(super) fn handle_account(
        &mut self,
        service: &ServiceType,
        cmd: u32,
        args: u64,
        tls: u64,
        mmu: &mut Memory,
    ) -> Option<u64> {
        Some(match service {
            ServiceType::Account => match cmd {
                100 | 140 => self.reply_ok(tls, mmu),
                0 => self.reply_data(tls, mmu, &1u32.to_le_bytes()),
                4 | 51 => self.reply_data(tls, mmu, &LOCAL_USER),
                5 => {
                    let mut uid = [0; 16];
                    if mmu.read_bytes(args, &mut uid).is_err() || uid != LOCAL_USER {
                        self.reply_error(tls, mmu, INVALID_ARGUMENT)
                    } else {
                        self.reply_object(tls, mmu, ServiceType::AccountProfile)
                    }
                }
                _ => self.reply_unimplemented("IAccountService", cmd, tls, mmu),
            },
            ServiceType::AccountProfile => match cmd {
                0 | 1 => {
                    let mut base = [0; 0x38];
                    base[..16].copy_from_slice(&LOCAL_USER);
                    base[24..34].copy_from_slice(b"Ounce User");
                    if cmd == 0 && self.write_out_buffer(mmu, &[0; 0x80]) != 0x80 {
                        self.reply_error(tls, mmu, INVALID_ARGUMENT)
                    } else {
                        self.reply_data(tls, mmu, &base)
                    }
                }
                _ => self.reply_unimplemented("IProfile", cmd, tls, mmu),
            },
            ServiceType::AccountLaunchStorage => match cmd {
                0 => self.reply_object(tls, mmu, ServiceType::AccountLaunchAccessor),
                _ => self.reply_unimplemented("IStorage", cmd, tls, mmu),
            },
            ServiceType::AccountLaunchAccessor => match cmd {
                0 => self.reply_data(tls, mmu, &0x88u64.to_le_bytes()),
                11 => {
                    let offset = mmu.read_u64(args).unwrap_or(u64::MAX);
                    let data = launch_parameter();
                    let buffer = self
                        .request
                        .recv_buffers
                        .iter()
                        .chain(&self.request.recv_list)
                        .find(|b| b.size > 0);
                    let size = buffer.map_or(u64::MAX, |b| b.size);
                    if offset
                        .checked_add(size)
                        .is_none_or(|end| end > data.len() as u64)
                    {
                        self.reply_error(tls, mmu, INVALID_ARGUMENT)
                    } else if self.write_out_buffer(mmu, &data[offset as usize..]) as u64 != size {
                        self.reply_error(tls, mmu, INVALID_ARGUMENT)
                    } else {
                        self.reply_ok(tls, mmu)
                    }
                }
                _ => self.reply_unimplemented("IStorageAccessor", cmd, tls, mmu),
            },
            _ => return None,
        })
    }

    pub(super) fn pop_account_launch_parameter(
        &mut self,
        kind: u32,
        tls: u64,
        mmu: &mut Memory,
    ) -> u64 {
        if kind != 2 || self.account_launch_consumed {
            return self.reply_error(tls, mmu, super::applet::RESULT_NO_DATA_IN_CHANNEL);
        }
        self.account_launch_consumed = true;
        self.reply_object(tls, mmu, ServiceType::AccountLaunchStorage)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kernel::cmif::test_util::{BUF, Harness, Req};

    #[test]
    fn local_account_launch_parameter_is_consumed_once() {
        let mut h = Harness::new();
        let functions = h.ipc.allocate_handle(ServiceType::Applet(
            super::super::applet::AppletInterface::ApplicationFunctions,
        ));
        let kind = 2u32.to_le_bytes();
        let launch = h.call(
            functions,
            Req {
                cmd: 1,
                data: &kind,
                ..Default::default()
            },
        );
        assert_eq!(launch.result, 0);
        let storage = launch.move_handles[0];
        assert_ne!(
            h.call(
                functions,
                Req {
                    cmd: 1,
                    data: &kind,
                    ..Default::default()
                }
            )
            .result,
            0
        );
        let accessor = h.call(storage, Req::default()).move_handles[0];
        assert_eq!(h.call_out(accessor, Req::default(), 8).u64(0), 0x88);
        let result = h.call(
            accessor,
            Req {
                cmd: 11,
                data: &0u64.to_le_bytes(),
                recv_list: Some((BUF, 0x88)),
                ..Default::default()
            },
        );
        assert_eq!(result.result, 0);
        let mut actual = [0; 0x88];
        h.mmu.read_bytes(BUF, &mut actual).unwrap();
        assert_eq!(actual, launch_parameter());
        let account = h.ipc.allocate_handle(ServiceType::Account);
        assert_ne!(
            h.call(
                account,
                Req {
                    cmd: 5,
                    data: &[0; 16],
                    ..Default::default()
                }
            )
            .result,
            0
        );
        let profile = h
            .call(
                account,
                Req {
                    cmd: 5,
                    data: &LOCAL_USER,
                    ..Default::default()
                },
            )
            .move_handles[0];
        let base = h.call_out(
            profile,
            Req {
                cmd: 0,
                recv_list: Some((BUF, 0x80)),
                ..Default::default()
            },
            0x38,
        );
        assert_eq!(base.result, 0);
        assert_eq!(&base.data[..16], &LOCAL_USER);
        assert_eq!(&base.data[24..34], b"Ounce User");
    }
}
