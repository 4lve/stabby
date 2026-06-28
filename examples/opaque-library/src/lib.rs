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

#[stabby::opaque(module = "examples::opaque_host")]
pub struct Host;

pub trait CounterApi {
    extern "C" fn get(&self) -> u32;
    extern "C" fn add(&mut self, amount: u32) -> u32;
    extern "C" fn label_len(&mut self, label: stabby::str::Str<'_>) -> u32;
}

#[stabby::interface(opaque = Host, prefix = "host")]
pub trait HostApi {
    extern "C" fn log(&mut self, message: stabby::str::Str<'_>);
    extern "C" fn increment_counter(&mut self, key: stabby::str::Str<'_>, amount: u32) -> u32;
}

#[stabby::interface(opaque = Host, prefix = "host_core", resolver)]
pub trait HostCore {
    extern "C" fn query_interface(
        &mut self,
        interface_id: u64,
        expected: &'static stabby::report::TypeReport,
    ) -> stabby::option::Option<stabby::opaque::ErasedInterfaceRefMut<Host>>;
}

struct CounterImpl {
    value: u32,
}

#[stabby::export]
pub extern "C" fn counter_new(value: u32) -> stabby::opaque::RefMut<Counter> {
    let counter = Box::leak(Box::new(CounterImpl { value }));
    unsafe { stabby::opaque::RefMut::from_mut(counter) }
}

#[stabby::export_interface(opaque = Counter, prefix = "counter")]
impl CounterApi for CounterImpl {
    extern "C" fn get(&self) -> u32 {
        self.value
    }

    extern "C" fn add(&mut self, amount: u32) -> u32 {
        self.value += amount;
        self.value
    }

    extern "C" fn label_len(&mut self, label: stabby::str::Str<'_>) -> u32 {
        label.len() as u32
    }
}
