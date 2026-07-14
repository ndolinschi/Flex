//! One model iteration: build request, stream response, materialize assistant.

mod finish;
mod retry;
mod run;
mod stream;

pub(super) use run::run_iteration;
