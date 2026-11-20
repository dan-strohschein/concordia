//! The native binding: a [`Module`] as a C ABI entry (used by [`export_module!`](crate::export_module)).

use std::marker::PhantomData;

use concordia_script_abi::Entry;
use concordia_script_protocol::codec::Call;
use concordia_script_protocol::{ModuleDesc, RunOutput};

use crate::Module;

/// `M` as a native module entry.
pub struct Adapter<M>(PhantomData<M>);

impl<M: Module> Entry for Adapter<M> {
    fn describe() -> ModuleDesc {
        M::describe()
    }

    fn run(call: &Call, log: &mut dyn FnMut(&str)) -> Result<RunOutput, String> {
        crate::run_module::<M>(&call.processor, call.tick, &call.batch, log)
    }
}
