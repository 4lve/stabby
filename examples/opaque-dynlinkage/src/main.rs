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

#[stabby::opaque(module = "examples::opaque_counter")]
pub struct Counter;

#[stabby::import(name = "opaque_library")]
extern "C" {
    pub fn counter_new(value: u32) -> stabby::opaque::RefMut<crate::Counter>;
}

#[stabby::import_interface(opaque = crate::Counter, prefix = "counter", name = "opaque_library")]
pub trait CounterApi {
    extern "C" fn get(&self) -> u32;
    extern "C" fn add(&mut self, amount: u32) -> u32;
    extern "C" fn label_len(&mut self, label: stabby::str::Str<'_>) -> u32;
}

struct HostImpl {
    counter: u32,
}

#[stabby::export_interface(
    opaque = opaque_library::Host,
    prefix = "host",
    vtable = opaque_library::HostApiVTable
)]
impl opaque_library::HostApi for HostImpl {
    extern "C" fn log(&mut self, message: stabby::str::Str<'_>) {
        self.counter += message.len() as u32;
    }

    extern "C" fn increment_counter(&mut self, key: stabby::str::Str<'_>, amount: u32) -> u32 {
        self.counter += key.len() as u32 + amount;
        self.counter
    }
}

#[stabby::export_interface(
    opaque = opaque_library::Host,
    prefix = "host_core",
    vtable = opaque_library::HostCoreVTable
)]
impl opaque_library::HostCore for HostImpl {
    extern "C" fn query_interface(
        &mut self,
        interface_id: u64,
        expected: &'static stabby::report::TypeReport,
    ) -> stabby::option::Option<stabby::opaque::ErasedInterfaceRefMut<opaque_library::Host>> {
        host_interface_query_impl(self, interface_id, expected)
    }
}

fn simulated_plugin_callback(
    mut host: opaque_library::HostCoreRefMut,
    player: stabby::str::Str<'_>,
) -> u32 {
    use opaque_library::{HostApi, HostApiInterfaceExt};

    let mut host = host.resolve_host_api().unwrap();
    host.log(stabby::str::Str::new("joined"));
    host.increment_counter(player, 1)
}

fn main() {
    let mut counter = counter_new(40);

    assert_eq!(counter.get(), 40);
    assert_eq!(counter.add(2), 42);
    let label = String::from("short-lived");
    assert_eq!(counter.label_len(stabby::str::Str::new(&label)), 11);

    let mut host = HostImpl { counter: 0 };
    let host = host_core_interface_bind_impl(&mut host);
    let player = String::from("runtime-player");
    assert_eq!(
        simulated_plugin_callback(host, stabby::str::Str::new(&player)),
        21
    );

    println!("opaque counter value: {}", counter.get());
}
