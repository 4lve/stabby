//
// Copyright (c) 2023 ZettaScale Technology
//
// This program and the accompanying materials are made available under the
// terms of the Eclipse Public License 2.0 which is available at
// http://www.eclipse.org/legal/epl-2.0, or the Apache License, Version 2.0
// which is available at https://www.apache.org/licenses/LICENSE-2.0.
//
// SPDX-License-Identifier: EPL-2.0 OR Apache-2.0
//
// Contributors:
//   Pierre Avital, <pierre.avital@me.com>
//

use crate as stabby;

#[stabby::opaque]
pub struct Store;

#[stabby::opaque]
pub struct OtherStore;

#[stabby::opaque]
pub struct Host;

struct StoreImpl {
    len: u32,
}

struct HostImpl {
    calls: u32,
}

trait StoreApi {
    extern "C" fn len(&self) -> u32;
    extern "C" fn bump(&mut self, by: u32) -> u32;
}

#[stabby::interface(opaque = Host, prefix = "host")]
trait HostApi {
    extern "C" fn log(&mut self, message: stabby::str::Str<'_>);
    extern "C" fn increment_counter(&mut self, key: stabby::str::Str<'_>, amount: u32) -> u32;
}

#[stabby::interface(opaque = Host, prefix = "host_core", resolver)]
trait HostCore {
    extern "C" fn query_interface(
        &mut self,
        interface_id: u64,
        expected: &'static stabby::report::TypeReport,
    ) -> stabby::option::Option<stabby::opaque::ErasedInterfaceRefMut<Host>>;
}

#[stabby::export_interface(opaque = Store, prefix = "store")]
impl StoreApi for StoreImpl {
    extern "C" fn len(&self) -> u32 {
        self.len
    }

    extern "C" fn bump(&mut self, by: u32) -> u32 {
        self.len += by;
        self.len
    }
}

#[stabby::export_interface(opaque = Host, prefix = "host", vtable = HostApiVTable)]
impl HostApi for HostImpl {
    extern "C" fn log(&mut self, message: stabby::str::Str<'_>) {
        self.calls += message.len() as u32;
    }

    extern "C" fn increment_counter(&mut self, key: stabby::str::Str<'_>, amount: u32) -> u32 {
        self.calls += key.len() as u32 + amount;
        self.calls
    }
}

#[stabby::export_interface(opaque = Host, prefix = "host_core", vtable = HostCoreVTable)]
impl HostCore for HostImpl {
    extern "C" fn query_interface(
        &mut self,
        interface_id: u64,
        expected: &'static stabby::report::TypeReport,
    ) -> stabby::option::Option<stabby::opaque::ErasedInterfaceRefMut<Host>> {
        let mut this = unsafe { stabby::opaque::RefMut::<Host>::from_mut(self) };
        host_interface_query(&mut this, interface_id, expected)
    }
}

#[test]
fn opaque_markers_are_part_of_handle_reports() {
    assert!(!<stabby::opaque::Ref<Store> as stabby::IStable>::REPORT
        .is_compatible(<stabby::opaque::Ref<OtherStore> as stabby::IStable>::REPORT));
    assert!(!<stabby::opaque::RefMut<Store> as stabby::IStable>::REPORT
        .is_compatible(<stabby::opaque::RefMut<OtherStore> as stabby::IStable>::REPORT));
}

#[test]
fn export_interface_methods_accept_opaque_handles() {
    let mut store = StoreImpl { len: 7 };
    let shared = unsafe { stabby::opaque::Ref::<Store>::from_ref(&store) };
    assert_eq!(store_len(shared), 7);

    let mut exclusive = unsafe { stabby::opaque::RefMut::<Store>::from_mut(&mut store) };
    assert_eq!(store_bump(exclusive.reborrow(), 5), 12);
    assert_eq!(store_len(exclusive.as_ref()), 12);
}

#[test]
fn interface_ref_mut_calls_runtime_bound_vtable() {
    fn plugin_callback(
        mut host: HostApiRefMut,
        player: stabby::str::Str<'_>,
    ) -> u32 {
        host.log(stabby::str::Str::new("joined"));
        host.increment_counter(player, 1)
    }

    let mut host = HostImpl { calls: 0 };
    let host = unsafe { stabby::opaque::RefMut::<Host>::from_mut(&mut host) };
    let host = host_interface_bind(host);
    let player = String::from("local-player");

    assert_eq!(plugin_callback(host, stabby::str::Str::new(&player)), 19);
}

#[test]
fn frozen_core_resolves_extension_interface() {
    fn plugin_callback(mut host: HostCoreRefMut, player: stabby::str::Str<'_>) -> u32 {
        use HostApiInterfaceExt;

        let mut host_api = host.resolve_host_api().unwrap();
        host_api.log(stabby::str::Str::new("joined"));
        host_api.increment_counter(player, 1)
    }

    let mut host = HostImpl { calls: 0 };
    let host = unsafe { stabby::opaque::RefMut::<Host>::from_mut(&mut host) };
    let host = host_core_interface_bind(host);
    let player = String::from("local-player");

    assert_eq!(plugin_callback(host, stabby::str::Str::new(&player)), 19);
}
