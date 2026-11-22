//! Background operations on worker threads (ADR-0004, ADR-0009): the UI starts a job, shows its progress
//! and answers its questions; the job never waits for the screen and the screen never waits for the disk.

pub mod attributes;
pub mod delete;
pub mod disk_threads;
pub mod elevation;
pub mod file_reader;
pub mod find;
pub mod folder_info;
pub mod folder_size;
pub mod held;
pub mod line_index;
pub mod listing;
pub mod task_clock;
pub mod task_volumes;
pub mod tasks;
pub mod transfer;
pub mod tree;
pub mod tree_cache;
pub mod tree_nodes;
pub mod tree_worker;
pub mod work;
