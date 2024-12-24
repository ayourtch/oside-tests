#![windows_subsystem = "console"]

use pyembed::{MainPythonInterpreter, OxidizedPythonInterpreterConfig};

// Various cargo features can be defined to install a custom global allocator
// for Rust.
//
// Note that this *only* controls Rust's allocator: the Python interpreter
// has its own memory allocator settings on the
// `pyembed::OxidizedPythonInterpreterConfig` that will need to be set in
// order to fully leverage a custom allocator.

#[cfg(feature = "global-allocator-jemalloc")]
#[global_allocator]
static GLOBAL: jemallocator::Jemalloc = jemallocator::Jemalloc;

#[cfg(feature = "global-allocator-mimalloc")]
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

#[cfg(feature = "global-allocator-snmalloc")]
#[global_allocator]
static GLOBAL: snmalloc_rs::SnMalloc = snmalloc_rs::SnMalloc;

// Include an auto-generated file defining a
// `fn default_python_config<'a>() -> pyembed::OxidizedPythonInterpreterConfig<'a>`
// which returns an `OxidizedPythonInterpreterConfig` derived by the PyOxidizer
// configuration file.
//
// If you do not want your application to use this generated file or wish
// to explicitly instantiate the `OxidizedPythonInterpreterConfig` used to
// initialize the embedded Python interpreter, simply remove this line and
// the call to `default_python_config()` below.
include!(env!("DEFAULT_PYTHON_CONFIG_RS"));

use crate::pymod::PyInit_string_sum;
use std::ffi::CString;

pub mod pymod;

use oside;
use clap::Parser as ClapParser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// This program does something useful, but its author needs to edit this.
/// Else it will be just hanging around forever
#[derive(Debug, Clone, ClapParser, Serialize, Deserialize)]
#[clap(version = "0.0.1", author = "Andrew Yourtchenko <ayourtch@gmail.com>")]
struct Opts {
    /// Scapy python string to execute
    #[clap(short, long, default_value = "Ether()/IP()/UDP()")]
    scapy_expr: String,

    /// Override options from this yaml/json file
    #[clap(short, long)]
    options_override: Option<String>,

    /// A level of verbosity, and can be used multiple times
    #[clap(short, long, parse(from_occurrences))]
    verbose: i32,
}

fn main() {
    let opts: Opts = Opts::parse();

    // allow to load the options, so far there is no good built-in way
    let opts = if let Some(fname) = &opts.options_override {
        if let Ok(data) = std::fs::read_to_string(&fname) {
            let res = serde_json::from_str(&data);
            if res.is_ok() {
                res.unwrap()
            } else {
                serde_yaml::from_str(&data).unwrap()
            }
        } else {
            opts
        }
    } else {
        opts
    };

    if opts.verbose > 4 {
        let data = serde_json::to_string_pretty(&opts).unwrap();
        println!("{}", data);
        println!("===========");
        let data = serde_yaml::to_string(&opts).unwrap();
        println!("{}", data);
    }

    println!("Hello, here is your options: {:#?}", &opts);


    // The following code is in a block so the MainPythonInterpreter is destroyed in an
    // orderly manner, before process exit.
    let exit_code = {
        // Load the default Python configuration as derived by the PyOxidizer config
        // file used at build time.
        let mut config: OxidizedPythonInterpreterConfig = default_python_config();
        config.extra_extension_modules = Some(vec![pyembed::ExtensionModule {
            name: CString::new("string_sum").unwrap(),
            init_func: PyInit_string_sum,
        }]);

        // Construct a new Python interpreter using that config, handling any errors
        // from construction.
        match MainPythonInterpreter::new(config) {
            Ok(interp) => {
                // And run it using the default run configuration as specified by the
                // configuration.
                //
                // This will either call `interp.py_runmain()` or
                // `interp.run_multiprocessing()`. If `interp.py_runmain()` is called,
                // the interpreter is guaranteed to be finalized.
                println!("About to run with scapy loaded");
                // let dict: pyo3::types::PyDict = Default::default();
                interp.with_gil(|py| {
                    match py.run(
                        "import scapy; from scapy.all import *; a=IP(); a.show()",
                        None,
                        None,
                    ) {
                        Ok(_) => {
                            println!("python code executed successfully");
                        }
                        Err(e) => println!("python error: {:?}", e),
                    }
                    let x: Vec<u8> = py
                        .eval(&format!("bytes({})", &opts.scapy_expr), None, None)
                        .unwrap()
                        .extract()
                        .unwrap();
                    println!("X: {:02x?}", x);
                    {
                        use oside::*;
                        use oside::protocols::all::ether;

                        let x = Ether!().decode(&x);
                        println!("{:?}", &x);
                    }
                    // py.run("from scapy.main import interact; interact()", None, None);
                    
                });
                // interp.run()
                0
            }
            Err(msg) => {
                eprintln!("error instantiating embedded Python interpreter: {}", msg);
                1
            }
        }
    };

    // And exit the process according to code execution results.
    std::process::exit(exit_code);
}
