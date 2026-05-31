import re

with open("orch8-api/src/instances.rs", "r") as f:
    content = f.read()

old_str = """pub use checkpoints::{PruneCheckpointsRequest, SaveCheckpointRequest};
pub(crate) use checkpoints::{
    __path_get_latest_checkpoint, __path_list_checkpoints, __path_prune_checkpoints,
    __path_save_checkpoint, get_latest_checkpoint, list_checkpoints, prune_checkpoints,
    save_checkpoint,
};"""

new_str = """pub(crate) use checkpoints::{
    __path_get_latest_checkpoint, __path_list_checkpoints, __path_prune_checkpoints,
    __path_save_checkpoint, get_latest_checkpoint, list_checkpoints, prune_checkpoints,
    save_checkpoint,
};
pub use checkpoints::{PruneCheckpointsRequest, SaveCheckpointRequest};"""

content = content.replace(old_str, new_str)

with open("orch8-api/src/instances.rs", "w") as f:
    f.write(content)
