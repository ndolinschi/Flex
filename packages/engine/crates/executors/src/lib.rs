//! Command-execution backends implementing [`agentloop_core::Executor`].
//!
//! Each backend shells out to its CLI (`docker`, `ssh`, `apptainer`, …) via
//! `tokio::process`, mirroring how the workspace crate shells out to `git`:
//! implementation crates are the sanctioned I/O edges, and owning the process
//! invocation keeps heavyweight client libraries out of the tree.
//!
//! [`LocalExecutor`] is the default and is byte-compatible with the historical
//! in-tool spawn path: `/bin/sh -lc` in the session cwd.

mod container_image;
mod docker;
mod local;
mod process_group;
mod remote_fn;
mod run;
mod ssh;
mod win_console;

pub use container_image::ContainerImageExecutor;
pub use docker::DockerExecutor;
pub use local::LocalExecutor;
pub use remote_fn::RemoteFnExecutor;
pub use ssh::SshExecutor;
