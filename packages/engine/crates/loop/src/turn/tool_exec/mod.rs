//! Tool-call batching, permissions, hooks, and result feed-back.

mod batch;
mod dispatch;
mod intercept;
mod one_call;
mod permission;

pub(crate) use batch::MAX_CHILDREN_PER_TURN;
pub(crate) use batch::execute_tool_requests;
