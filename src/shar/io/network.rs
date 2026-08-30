//! Unbuilt stub, reserved for the actual peer-to-peer networking layer (tracking
//! known replica ids, connections, message transport). Everything here today —
//! `SharQueue::add_network_operation`/`remove_network_operation` — assumes an
//! `AddOperation`/`RemoveOperation` has already arrived from somewhere; nothing
//! yet actually sends or receives one over a network. See the shario roadmap.

// All replica IDs
