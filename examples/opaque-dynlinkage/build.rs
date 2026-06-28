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

fn main() {
    let Ok(dir) = std::env::var("PROFILE") else {
        return;
    };
    let profile_dir = [".", "target", &dir]
        .into_iter()
        .collect::<std::path::PathBuf>();
    println!(
        "cargo:rustc-link-search=native={}",
        profile_dir.to_str().unwrap()
    );
    println!(
        "cargo:rustc-link-search=native={}",
        profile_dir.join("deps").to_str().unwrap()
    );
    let target = std::env::var("TARGET").unwrap_or_default();
    if target.contains("apple") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,@executable_path/deps");
    } else if target.contains("linux") || target.contains("bsd") {
        println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN/deps");
    }
}
